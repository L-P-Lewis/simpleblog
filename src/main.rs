use http::StatusCode;
use poem::{
    endpoint::StaticFilesEndpoint,
    get, handler,
    listener::TcpListener,
    web::{
        headers::{authorization::Basic, Authorization},
        Data, Json, Path, Query, TypedHeader,
    },
    EndpointExt, Response, Route, Server,
};
use serde::{Deserialize, Serialize};
use std::{
    env,
    io::{ErrorKind, Read, Write},
};
// Simpleblog by Luke Lewis
//
// A minimal poem implementation of a blog website, complete with an article list, homepage, and RSS feed

// STRUCTS

// Struct representing site configuration, read in from site_config.yml on startup
#[derive(Debug, Deserialize, Clone)]
struct SiteConfig {
    port: String,
    file_path: String,
    site_title: String,
    site_description: String,
    site_link: String,
    admin_username: String,
    admin_password: String,
}

// Struct for representing a url query representing the page on the articles list
#[derive(Deserialize)]
struct ArticleIndex {
    index: Option<u16>,
}

// A struct representing an article
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
struct Article {
    title: String,
    article_id: String,
    description: String,
    date: String,
}

impl Article {
    // Code to build an HTML element representing an article
    fn to_preview_html(&self) -> String {
        format!(
            "
            <div class='article_preview'>
                <h2>{title}</h2>
                <div class='preview_content'>
                <p class='article_timestamp'>{date}</p>
                <p>{description}</p>
                </div>
                <a href='/./articles/{article_id}'>Read</a>
            </div>
            ",
            title = self.title,
            date = self.date,
            description = self.description,
            article_id = self.article_id
        )
    }
    // Code to convert an article's data into XML form in RSS specification
    fn to_preview_xml(&self, config: &SiteConfig) -> String {
        format!(
            "
            <item>
                <title>{title}</title>
                <pubDate>{date}</pubDate>
                <description>{description}</description>
                <link>{site_path}/articles/{article_id}</link>
            </item>
            ",
            title = self.title,
            date = self.date,
            description = self.description,
            article_id = self.article_id,
            site_path = config.site_link
        )
    }
}

// Code for ordering articles by post date.  At the moment posts are sorted alphebetically, and it is expected that the date be written in yyyy-mm-dd format.
impl PartialOrd for Article {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        other.date.partial_cmp(&self.date)
    }
}

impl Ord for Article {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.date.cmp(&self.date)
    }
}

// ENDPOINT HANDLERS

// Endpoint handler for the homepage. Builds a static page from index.html, with the latest article inserted
#[handler]
fn homepage(filepath: Data<&String>) -> Response {
    let mut index_target: String = filepath.0.to_string();
    index_target.push_str("index.html");

    let mut index_file = match std::fs::File::open(index_target) {
        Ok(n) => n,
        Err(_) => {
            return get_404_error(filepath);
        }
    };
    let mut index_contents = String::new();
    match index_file.read_to_string(&mut index_contents) {
        Ok(_) => {}
        Err(_) => {
            return get_404_error(filepath);
        }
    };

    let mut article_list: Vec<Article> = match get_articles(&filepath) {
        Ok(a) => a,
        _ => {
            return get_404_error(filepath);
        }
    };
    article_list.sort();

    let article_elements: Vec<String> = article_list
        .iter()
        .take(1)
        .map(|a| a.to_preview_html())
        .collect();

    for element in article_elements {
        index_contents = index_contents.replace("{latest_article}", &element);
    }

    poem::Response::builder()
        .status(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(index_contents)
}

// Handler for an article page. Builds from the article_template.html page and inserts converted markdown
#[handler]
fn article(Path(article_id): Path<String>, filepath: Data<&String>) -> Response {
    let mut article_target: String = filepath.0.to_string();
    article_target.push_str("articles/");
    article_target.push_str(&article_id);
    article_target.push_str(".md");

    let mut base_target: String = filepath.0.to_string();
    base_target.push_str("article_template.html");
    let mut base_file = match std::fs::File::open(base_target) {
        Ok(f) => f,
        Err(_) => {
            return get_404_error(filepath);
        }
    };
    let mut base_contents = String::new();
    match base_file.read_to_string(&mut base_contents) {
        Err(_) => {
            return get_404_error(filepath);
        }
        _ => {}
    };

    let target_path = std::path::Path::new(&article_target);

    let article_content = match markdown::file_to_html(&target_path) {
        Ok(c) => c,
        _ => {
            return get_404_error(filepath);
        }
    };

    let mut final_content = base_contents.replace("{article_content}", &article_content);

    poem::Response::builder()
        .status(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(final_content)
}

// Handler for articles list. Builds a paginated list of ten articles at a time, and inserts nav buttons when applicable
#[handler]
fn articles(
    filepath: Data<&String>,
    Query(ArticleIndex { index }): Query<ArticleIndex>,
) -> Response {
    let true_index = match index {
        Some(i) => i,
        _ => 0,
    };

    let mut articles: Vec<Article> = match get_articles(&filepath) {
        Ok(a) => a,
        _ => {
            return get_404_error(filepath);
        }
    };
    articles.sort();

    let num_articles: u16 = articles.len().try_into().unwrap();
    let num_pages = num_articles / 10;

    let article_elements: Vec<String> = articles
        .iter()
        .skip(usize::from(true_index * 10))
        .take(10)
        .map(|a| a.to_preview_html())
        .collect();

    let mut content: String = String::new();
    for element in article_elements {
        content.push_str(&element);
    }

    let mut base_target: String = filepath.0.to_string();
    base_target.push_str("articles.html");
    let mut base_file = match std::fs::File::open(base_target) {
        Ok(f) => f,
        Err(_) => {
            return get_404_error(filepath);
        }
    };
    let mut base_contents = String::new();
    match base_file.read_to_string(&mut base_contents) {
        Err(_) => {
            return get_404_error(filepath);
        }
        _ => {}
    };

    base_contents = base_contents.replace("{articles}", &content);


    let mut nav_buttons = String::new();
    nav_buttons.push_str("<ul class = 'article_bar'>");
    if true_index != 0 {
        nav_buttons.push_str(&format!("<li><a href=articles?index=0>First</a></li>"));
        nav_buttons.push_str(&format!(
            "<li><a href=articles?index={}>Previous</a></li>",
            true_index - 1
        ));
    }
    if true_index < num_pages {
        nav_buttons.push_str(&format!(
            "<li><a href=articles?index={}>Next</a></li>",
            true_index + 1
        ));
        nav_buttons.push_str(&format!("<li><a href=articles?index={}>Last</a></li>", num_pages));
    }
    nav_buttons.push_str("</ul>");

    base_contents = base_contents.replace("{links}", &nav_buttons);

    poem::Response::builder()
        .status(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(base_contents)
}

// Post function. Ads an article to the articles.yml list if the sender has the correct auth
#[handler]
async fn post_article(
    filepath: Data<&String>,
    Data(config): Data<&SiteConfig>,
    Json(article_data): Json<Article>,
    TypedHeader(auth): TypedHeader<Authorization<Basic>>,
) -> StatusCode {
    let true_username = config.admin_username.as_str();
    if !auth.username().eq(true_username) {
        return StatusCode::METHOD_NOT_ALLOWED;
    }

    let true_password = config.admin_password.as_str();
    if !auth.password().eq(true_password) {
        return StatusCode::METHOD_NOT_ALLOWED;
    }

    let mut article_target: String = filepath.0.to_string();
    article_target.push_str("articles.yml");
    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .append(true)
        .open(article_target)
    {
        Ok(f) => f,
        _ => {
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
    };

    let serialized_data = match serde_yml::to_string(&article_data) {
        Ok(d) => d,
        _ => {
            return StatusCode::BAD_REQUEST;
        }
    };

    match file.write(serialized_data.as_bytes()) {
        Ok(_) => {}
        Err(_) => {
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
    };

    return StatusCode::OK;
}

// Gets the RSS feed for the blog. Returns a RSS 2.0 compliant xml object of the last ten articles
#[handler]
async fn get_feed(filepath: Data<&String>, config: Data<&SiteConfig>) -> Response {
    let mut prev_articles: Vec<Article> = match get_articles(&filepath) {
        Ok(a) => a,
        _ => {
            return get_404_error(filepath);
        }
    };
    prev_articles.sort();
    let article_elements: Vec<String> = prev_articles
        .iter()
        .take(10)
        .map(|a| a.to_preview_xml(config.0))
        .collect();

    let mut content: String = String::new();
    for element in article_elements {
        content.push_str(&element);
    }

    poem::Response::builder()
        .status(StatusCode::OK)
        .content_type("text/xml; charset=utf-8")
        .body(format!(
            "
        <rss version=\"2.0\">
        <channel>
        <title>{title}</title>
        <link>{link}</link>
        <description>{description}</description>
        {content}
        </channel>
        </rss>
        ",
            title = config.0.site_title,
            link = config.0.site_link,
            description = config.0.site_description
        ))
}

// HELPER FUNCTIONS

// Gets the 404 page at fnfpage.html, or builds a default one if that doesn't exist
fn get_404_error(filepath: Data<&String>) -> Response {
    let mut index_target: String = filepath.0.to_string();
    index_target.push_str("fnfpage.html");

    let mut index_file = match std::fs::File::open(index_target) {
        Ok(f) => f,
        Err(_) => {
            return poem::Response::builder()
                .status(StatusCode::NOT_FOUND)
                .content_type("text/html; charset=utf-8")
                .body("<h1>404 Page not found</h1><p>Ironic I know</p>")
        }
    };
    let mut index_contents = String::new();
    index_file.read_to_string(&mut index_contents).unwrap();

    poem::Response::builder()
        .status(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(index_contents)
}

// Helper Function, gets a list of all articles in articles.yml
fn get_articles(filepath: &Data<&String>) -> Result<Vec<Article>, ()> {
    let mut article_target: String = filepath.0.to_string();
    article_target.push_str("articles.yml");

    let mut base_file = match std::fs::File::open(article_target) {
        Ok(f) => f,
        Err(_) => {
            return Err(());
        }
    };
    let mut base_contents = String::new();
    match base_file.read_to_string(&mut base_contents) {
        Err(_) => {
            return Err(());
        }
        _ => {}
    };

    let out: Vec<Article> = match serde_yml::from_str(&base_contents) {
        Ok(o) => o,
        Err(_) => {
            return Err(());
        }
    };

    return Ok(out);
}

// MAIN FUNCTION
#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let args: Vec<String> = env::args().collect();
    let config_file_path = &args[1];

    println!("Starting server with config file at {:?}", config_file_path);

    let mut cfg_file = match std::fs::File::open(config_file_path) {
        Ok(f) => f,
        Err(_) => {
            println!("Error finding config file");
            return Err(std::io::Error::from(ErrorKind::InvalidData));
        }
    };
    let mut cfg_contents = String::new();
    match cfg_file.read_to_string(&mut cfg_contents) {
        Err(_) => {
            println!("Error reading config file");
            return Err(std::io::Error::from(ErrorKind::InvalidData));
        }
        _ => {}
    };
    let config: SiteConfig = match serde_yml::from_str(&cfg_contents) {
        Ok(f) => f,
        Err(_) => {
            println!("Error parsing config file");
            return Err(std::io::Error::from(ErrorKind::InvalidData));
        }
    };

    let path = config.file_path.clone();

    let app = Route::new()
        .at("", get(homepage))
        .at("articles", get(articles).post(post_article))
        .at("articles/:article_id", get(article))
        .at("feed", get(get_feed))
        .nest(
            "/assets",
            StaticFilesEndpoint::new(format!("{}/assets", config.file_path)),
        )
        .data(path)
        .data(config.clone());

    Server::new(TcpListener::bind(config.port))
        .run(app)
        .await
}
