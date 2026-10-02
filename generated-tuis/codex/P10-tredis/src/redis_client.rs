use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

#[derive(Clone, Debug)]
pub enum Resp {
    Simple(String),
    Error(String),
    Integer(i64),
    Bulk(Option<Vec<u8>>),
    Array(Option<Vec<Resp>>),
}

impl Resp {
    pub fn text(&self) -> String {
        match self {
            Resp::Simple(s) | Resp::Error(s) => s.clone(),
            Resp::Integer(i) => i.to_string(),
            Resp::Bulk(Some(v)) => String::from_utf8_lossy(v).into_owned(),
            Resp::Bulk(None) | Resp::Array(None) => "(nil)".into(),
            Resp::Array(Some(v)) => v.iter().map(Resp::text).collect::<Vec<_>>().join(" "),
        }
    }
    pub fn into_array(self) -> Result<Vec<Resp>, RedisError> {
        match self {
            Resp::Array(Some(v)) => Ok(v),
            Resp::Array(None) => Ok(Vec::new()),
            Resp::Error(e) => Err(RedisError(e)),
            other => Err(RedisError(format!("expected array, got {other:?}"))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RedisError(pub String);
impl fmt::Display for RedisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for RedisError {}
impl From<std::io::Error> for RedisError {
    fn from(value: std::io::Error) -> Self {
        Self(value.to_string())
    }
}

#[derive(Clone, Debug)]
pub struct RedisUri {
    pub host: String,
    pub port: u16,
    pub db: u32,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl RedisUri {
    pub fn parse(uri: &str) -> Result<Self, RedisError> {
        let raw = uri
            .strip_prefix("redis://")
            .ok_or_else(|| RedisError("URI must begin with redis://".into()))?;
        let (authority, path) = raw.split_once('/').unwrap_or((raw, "0"));
        let (auth, hostport) = match authority.rsplit_once('@') {
            Some((a, h)) => (Some(a), h),
            None => (None, authority),
        };
        let (username, password) = match auth {
            Some(a) => match a.split_once(':') {
                Some((u, p)) => (
                    if u.is_empty() {
                        None
                    } else {
                        Some(percent_decode(u)?)
                    },
                    Some(percent_decode(p)?),
                ),
                None => (None, Some(percent_decode(a)?)),
            },
            None => (None, None),
        };
        let (host, port) = if hostport.starts_with('[') {
            let end = hostport
                .find(']')
                .ok_or_else(|| RedisError("invalid IPv6 host".into()))?;
            let host = hostport[1..end].to_string();
            let tail = &hostport[end + 1..];
            let port = if tail.is_empty() {
                6379
            } else {
                tail.strip_prefix(':')
                    .ok_or_else(|| RedisError("invalid port".into()))?
                    .parse()
                    .map_err(|_| RedisError("invalid port".into()))?
            };
            (host, port)
        } else {
            match hostport.rsplit_once(':') {
                Some((h, p)) if !h.is_empty() => (
                    h.to_string(),
                    p.parse().map_err(|_| RedisError("invalid port".into()))?,
                ),
                _ => (hostport.to_string(), 6379),
            }
        };
        if host.is_empty() {
            return Err(RedisError("missing Redis host".into()));
        }
        let db_text = path.split(['?', '#']).next().unwrap_or("0");
        let db = if db_text.is_empty() {
            0
        } else {
            db_text
                .parse()
                .map_err(|_| RedisError("invalid database number".into()))?
        };
        Ok(Self {
            host,
            port,
            db,
            username,
            password,
        })
    }
}

fn percent_decode(s: &str) -> Result<String, RedisError> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(RedisError("invalid percent escape in URI".into()));
            }
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3])
                .map_err(|_| RedisError("invalid URI".into()))?;
            out.push(
                u8::from_str_radix(hex, 16)
                    .map_err(|_| RedisError("invalid percent escape in URI".into()))?,
            );
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| RedisError("URI credentials are not UTF-8".into()))
}

pub struct RedisClient {
    reader: BufReader<TcpStream>,
}

impl RedisClient {
    pub fn connect(uri: &str) -> Result<Self, RedisError> {
        let config = RedisUri::parse(uri)?;
        let addr = (config.host.as_str(), config.port)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| RedisError("could not resolve Redis host".into()))?;
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(4))?;
        stream.set_read_timeout(Some(Duration::from_secs(15)))?;
        stream.set_write_timeout(Some(Duration::from_secs(15)))?;
        let mut client = Self {
            reader: BufReader::new(stream),
        };
        if let Some(password) = config.password {
            if let Some(username) = config.username {
                client.command(&["AUTH", &username, &password])?;
            } else {
                client.command(&["AUTH", &password])?;
            }
        }
        if config.db != 0 {
            client.command(&["SELECT", &config.db.to_string()])?;
        }
        client.command(&["PING"])?;
        Ok(client)
    }

    pub fn command(&mut self, args: &[&str]) -> Result<Resp, RedisError> {
        let mut payload = Vec::new();
        write!(&mut payload, "*{}\r\n", args.len())?;
        for arg in args {
            write!(&mut payload, "${}\r\n", arg.as_bytes().len())?;
            payload.extend_from_slice(arg.as_bytes());
            payload.extend_from_slice(b"\r\n");
        }
        self.reader.get_mut().write_all(&payload)?;
        self.reader.get_mut().flush()?;
        let response = read_resp(&mut self.reader)?;
        if let Resp::Error(e) = response {
            Err(RedisError(e))
        } else {
            Ok(response)
        }
    }
}

fn read_resp<R: BufRead>(reader: &mut R) -> Result<Resp, RedisError> {
    let mut marker = [0u8; 1];
    reader.read_exact(&mut marker)?;
    match marker[0] {
        b'+' => Ok(Resp::Simple(read_line(reader)?)),
        b'-' => Ok(Resp::Error(read_line(reader)?)),
        b':' => {
            Ok(Resp::Integer(read_line(reader)?.parse().map_err(|_| {
                RedisError("invalid integer response".into())
            })?))
        }
        b'$' => {
            let len: i64 = read_line(reader)?
                .parse()
                .map_err(|_| RedisError("invalid bulk length".into()))?;
            if len < 0 {
                return Ok(Resp::Bulk(None));
            }
            let mut data = vec![0; len as usize];
            reader.read_exact(&mut data)?;
            let mut crlf = [0; 2];
            reader.read_exact(&mut crlf)?;
            Ok(Resp::Bulk(Some(data)))
        }
        b'*' => {
            let len: i64 = read_line(reader)?
                .parse()
                .map_err(|_| RedisError("invalid array length".into()))?;
            if len < 0 {
                return Ok(Resp::Array(None));
            }
            let mut values = Vec::with_capacity(len as usize);
            for _ in 0..len {
                values.push(read_resp(reader)?);
            }
            Ok(Resp::Array(Some(values)))
        }
        b'_' => {
            let _ = read_line(reader)?;
            Ok(Resp::Bulk(None))
        }
        b'#' | b',' | b'(' => Ok(Resp::Simple(read_line(reader)?)),
        b'!' | b'=' => {
            let len: usize = read_line(reader)?
                .parse()
                .map_err(|_| RedisError("invalid RESP3 length".into()))?;
            let mut data = vec![0; len];
            reader.read_exact(&mut data)?;
            let mut crlf = [0; 2];
            reader.read_exact(&mut crlf)?;
            Ok(Resp::Bulk(Some(data)))
        }
        b'%' | b'~' | b'>' => {
            let len: usize = read_line(reader)?
                .parse()
                .map_err(|_| RedisError("invalid RESP3 collection length".into()))?;
            let count = if marker[0] == b'%' { len * 2 } else { len };
            let mut values = Vec::with_capacity(count);
            for _ in 0..count {
                values.push(read_resp(reader)?);
            }
            Ok(Resp::Array(Some(values)))
        }
        other => Err(RedisError(format!(
            "unknown RESP marker {:?}",
            other as char
        ))),
    }
}

fn read_line<R: BufRead>(reader: &mut R) -> Result<String, RedisError> {
    let mut line = String::new();
    reader.read_line(&mut line)?;
    if !line.ends_with("\r\n") {
        return Err(RedisError("truncated Redis response".into()));
    }
    line.truncate(line.len() - 2);
    Ok(line)
}
