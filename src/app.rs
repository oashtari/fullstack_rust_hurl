use log::{debug, trace};
use std::convert::TryFrom;
use structopt::StructOpt;

use crate::errors::{Error, HurlResult};

/// A command line HTTP client
#[derive(StructOpt, Debug)]
#[structopt(name = "hurl")]
pub struct App {
        /// Activate quiet mode.
    ///
    /// This overrides any verbose settings.
    #[structopt(short, long)]
    pub quiet: bool,

    /// Verbose mode (-v, -vv, -vvv, etc.).
    #[structopt(short, long, parse(from_occurrences))]
    pub verbose: u8,

    /// Form mode.
    #[structopt(short, long)]
    pub form: bool,

    /// Basic authentication.
    ///
    /// A string of the form `username:password`. If only
    /// `username` is given then you will be prompted
    /// for a password. If you wish to use no password
    /// then use the form `username:`.
    #[structopt(short, long)]
    pub auth: Option<String>,

    /// Bearer token authentication.
    ///
    /// A token which will be sent as "Bearaer <token>" in
    /// the authorization header.
    #[structopt(short, long)]
    pub token: Option<String>,

    /// Default transport.
    ///
    /// If a URL is given without a transport, i.e example.com/foo
    /// http will be used as the transport by default. If this flag
    /// is set then https will be used instead.
    #[structopt(short, long)]
    pub secure: bool,

    /// The HTTP Method to use, one of: HEAD, GET, POST, PUT, PATCH, DELETE.
    #[structopt(subcommand)]
    pub cmd: Option<Method>,

    /// The URL to issue a request to if a method subcommand is not specified.
    pub url: Option<String>,

    /// The parameters for the request if a method subcommand is not specified.
    ///
    /// There are seven types of parameters that can be added to a command-line.
    /// Each type of parameter is distinguished by the unique separator between
    /// the key and value.
    ///
    /// Header -- key:value
    ///
    ///   e.g. X-API-TOKEN:abc123
    ///
    /// File upload -- key@filename
    ///
    ///   this simulates a file upload via multipart/form-data and requires --form
    ///
    /// Query parameter -- key==value
    ///
    ///   e.g. foo==bar becomes example.com?foo=bar
    ///
    /// Data field -- key=value
    ///
    ///   e.g. foo=bar becomes {"foo":"bar"} for JSON or form encoded
    ///
    /// Data field from file -- key=@filename
    ///
    ///   e.g. foo=@bar.txt becomes {"foo":"the contents of bar.txt"} or form encoded
    ///
    /// Raw JSON data where the value should be parsed to JSON first -- key:=value
    ///
    ///   e.g. foo:=[1,2,3] becomes {"foo":[1,2,3]}
    ///
    /// Raw JSON data from file -- key:=@filename
    ///
    ///   e.g. foo:=@bar.json becomes {"foo":{"bar":"this is from bar.json"}}
    #[structopt(parse(try_from_str = parse_param))]
    pub parameters: Vec<Parameter>,
}

impl App {
    pub fn validate(&mut self) -> HurlResult<()> {
        if self.cmd.is_none() && self.url.is_none() {
            return Err(Error::MissingUrlAndCommand);
        }
        Ok(())
    }

    pub fn log_level(&self) -> Option<&'static str> {
        if self.quiet || self.verbose <= 0 {
            return None;
        }

        match self.verbose {
            1 => Some("error"),
            2 => Some("warn"),
            3 => Some("info"),
            4 => Some("debug"),
            _ => Some("trace"),
        }
    }
}

impl Method {
    pub fn data(&self) -> &MethodData {
        use Method::*;
        match self {
            HEAD(x) => x,
            GET(x) => x,
            PUT(x) => x,
            POST(x) => x,
            PATCH(x) => x,
            DELETE(x) => x,
        }
    }
}

impl From<&Method> for reqwest::Method {
    fn from(m: &Method) -> reqwest::Method {
        match m {
            Method::HEAD(_) => reqwest::Method::HEAD,
            Method::GET(_) => reqwest::Method::GET,
            Method::PUT(_) => reqwest::Method::PUT,
            Method::POST(_) => reqwest::Method::POST,
            Method::PATCH(_) => reqwest::Method::PATCH,
            Method::DELETE(_) => reqwest::Method::DELETE,
        }
    }
}

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "screaming_snake_case")]
pub enum Method {
    HEAD(MethodData),
    GET(MethodData),
    PUT(MethodData),
    POST(MethodData),
    PATCH(MethodData),
    DELETE(MethodData),
}

#[derive(StructOpt, Debug)]
pub struct MethodData {
    /// The URL to request.
    pub url: String,

    /// The headers, data, and query parameters to add to the request.
    /// 
    ///     #[structopt(parse(try_from_str = parse_param))]
    pub parameters: Vec<Parameter>,
}

#[derive(Debug)]
pub enum Parameter {
    // :
    Header { key: String, value: String },
    // =
    Data { key: String, value: String },
    // :=
    RawJsonData { key: String, value: String },
    // ==
    Query { key: String, value: String },
    // @
    FormFile { key: String, filename: String },
    // =@
    DataFile { key: String, filename: String },
    // :=@
    RawJsonDataFile { key: String, filename: String },
}

impl Parameter {
    pub fn is_form_file(&self) -> bool {
        match *self {
            Parameter::FormFile { .. } => true,
            _ => false,
        }
    }

    pub fn is_data(&self) -> bool {
        match *self {
            Parameter::Header { .. } => false,
            Parameter::Query { .. } => false,
            _ => true,
        }
    }
}

#[derive(Debug)]
enum Separator {
    Colon,
    Equal,
    At,
    ColonEqual,
    EqualEqual,
    EqualAt,
    Snail,
}

impl TryFrom<&str> for Separator {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            ":" => Ok(Separator::Colon),
            "=" => Ok(Separator::Equal),
            "@" => Ok(Separator::At),
            ":=" => Ok(Separator::ColonEqual),
            "==" => Ok(Separator::EqualEqual),
            "=@" => Ok(Separator::EqualAt),
            ":=@" => Ok(Separator::Snail),
            _ => Err(()),
        }
    }
}

#[derive(Debug)]
enum Token<'a> {
    Text(:'a str),
    Escape(char),
}

fn gather_escapes<'a>(src: :'a str) -> Vec<Token<'a>> {
    let mut tokens = Vec::new();
    let mut start = 0;
    let mut end = 0;
    let mut chars = src.chars();
    loop {
        let a = chars.next();
        if a.is_none() {
            if start != end {
                tokens.push(Token::Text(&src[start..end]));
            }
            return tokens;
        }
        let c = a.unwrap();
        if c != '\\' {
            end += 1;
            continue;
        }
        let b = chars.next();
        if b.is_none() {
            tokens.push(Token::Text(&src[start..end + 1]));
            return tokens;
        }
        let c = b.unwrap();
        match c {
            '\\' | '=' | '@' | ':' => {
                if start != end {
                    tokens.push(Token::Text(&src[start..end]));
                }
                tokens.push(Token::Escape(c));
                end += 2;
                start = end;
            }
            _ => end += 2,
        }
    }
}

fn parse_param(src: &str) -> HurlResult<Parameter> {
    debug!("Parsing: {}", src);
    let separators = [":=@", "=@", "==", ":=", "@", "=", ":"];
    let tokens = gather_escapes(src);

    let mut found = Vec::new();
    let mut idx = 0;
    for (i, token) in tokens.iter().enumerate() {
        match token {
            Token::Text(s) => {
                for sep in separators.iter() {
                    if let Some(n) = s.find(sep) {
                        found.push((n, sep));
                    }
                }
                if !found.is_empty() {
                    idx = i;
                    break;
                }
            }
            Token::Escape(_) => {}
        }
    }
    if found.is_empty() {
        return Err(Error::ParameterMissingSeparator(src.to_owned()));
    }
    found.sort_by(|(ai, asep), (bi, bsep)| ai.cmp(bi).then(bsep.len().cmp(&asep.len())));
    let sep = found.first().unwrap().1;
    trace!("Found separator: {}", sep);

    let mut key = String::new();
    let mut value = String::new();
    for (i, token) in tokens.iter().enumerate() {
        if i < idx {
            match token {
                Token::Text(s) => key.push_str(&s),
                Token::Escape(c) => {
                    key.push('\\');
                    key.push(*c);
                }
            }
        } else if i > idx {
            match token {
                Token::Text(s) => value.push_str(&s),
                Token::Escape(c) => {
                    value.push('\\');
                    value.push(*c);
                }
            }
        } else {
            if let Token::Text(s) = token {
                let parts: Vec<&str> = s.splitn(2, sep).collect();
                let k = parts.first().unwrap();
                let v = parts.last().unwrap();
                key.push_str(k);
                value.push_str(v);
            } else {
                unreachable!();
            }
        }
    }

    if let Ok(separator) = Separator::try_from(*sep) {
        match separator {
            Separator::At => Ok(Parameter::FormFile {
                key,
                filename: value,
            }),
            Separator::Equal => Ok(Parameter::Data { key, value }),
            Separator::Colon => Ok(Parameter::Header { key, value }),
            Separator::ColonEqual => Ok(Parameter::RawJsonData { key, value }),
            Separator::EqualEqual => Ok(Parameter::Query { key, value }),
            Separator::EqualAt => Ok(Parameter::DataFile {
                key,
                filename: value,
            }),
            Separator::Snail => Ok(Parameter::RawJsonDataFile {
                key,
                filename: value,
            }),
        }
    } else {
        unreachable!();
    }
}