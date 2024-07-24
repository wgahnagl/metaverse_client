use crate::models::simulator_login_protocol::{
    Login, SimulatorLoginOptions, SimulatorLoginProtocol,
};
use crate::models::util::*;
use hyper::header::CONTENT_TYPE;
use hyper::{Body, Client, Request};
use md5;
use regex::Regex;
use std::env;
use std::error::Error;

use mac_address::get_mac_address;
use md5::{Digest, Md5};
use std::fs::File;
use std::io::Read;

extern crate sys_info;

///Logs in using a SimulatorLoginProtocol object and the url string.
///returns a String containing the server's status code
pub async fn login(
    login_data: SimulatorLoginProtocol,
    url: String,
) -> Result<LoginResponse, Box<dyn Error>> {
    let req = xmlrpc::Request::new("login_to_simulator").arg(login_data);
    let xml = match clean_xml(req) {
        Ok(xml) => xml,
        Err(e) => return Err(format!("failed to log in: {}", e).into()),
    };

    let client = Client::new();

    // Create the HTTP request
    let req = Request::builder()
        .method(hyper::Method::POST)
        .uri(url)
        .header(CONTENT_TYPE, "text/xml")
        .body(Body::from(xml))
        .expect("Failed to build request");

    // Send the request
    let res = client.request(req).await?;

    let status_code = res.status().as_u16().to_string();
    let body_bytes = hyper::body::to_bytes(res.into_body()).await?;
    let body = String::from_utf8(body_bytes.to_vec())?;

    Ok(LoginResponse { status_code, body })
}

/// this cleans the XML and makes it usable by the simulator.
/// this is a workaround for the limitations of xmlrpc, which randomly converts ints to i4 and i8.
/// The simulator protocol requires only ints.
///```
///use metaverse_login:: login::{clean_xml};
///let login_data = build_login(Login {
///    first: "default".to_string(),
///    last: "user".to_string(),
///    channel: "benthic".to_string(),
///    agree_to_tos: true,
///    read_critical: true,
///});
///
///let xml_request = Request::new("login_to_simulator").arg(login_data);
///let clean_xml_result = clean_xml(&xml_request).expect("Failed to clean XML");
///
///assert!(!clean_xml_result.contains("<i4>"));
///assert!(!clean_xml_result.contains("</i4>"));
///```
pub fn clean_xml(xml: xmlrpc::Request) -> Result<String, Box<dyn Error>> {
    let mut output: Vec<u8> = vec![];
    xml.write_as_xml(&mut output)?;
    let request_string = String::from_utf8(output).map_err(|e| Box::new(e) as Box<dyn Error>)?;

    let re_i4 = Regex::new(r"</?i4>").unwrap();
    let re_i8 = Regex::new(r"</?i8>").unwrap();

    // Replace i4 and i8 with int
    let result = re_i4.replace_all(&request_string, |caps: &regex::Captures| {
        if &caps[0] == "<i4>" {
            "<int>"
        } else {
            "</int>"
        }
    });

    let final_string = re_i8
        .replace_all(&result, |caps: &regex::Captures| {
            if &caps[0] == "<i8>" {
                "<int>"
            } else {
                "</int>"
            }
        })
        .into_owned();

    Ok(final_string)
}

///Generates a SimulatorLoginProtocol based on user supplied values
///returns a SimulatorLoginProtocol
///```
///use metaverse_login::login::{build_struct_with_defaults};
///
///let login_struct = build_login(Login{
///                         first: "default".to_string(),
///                         last: "user".to_string(),
///                         start: "home".to_string(),
///                         channel: "benthic".to_string(),
///                         agree_to_tos: true,
///                         read_critical: true
///                         });
///assert_eq!(login_struct.first, "first");
pub fn build_login(login: Login) -> SimulatorLoginProtocol {
    SimulatorLoginProtocol {
        first: login.first,
        last: login.last,
        passwd: hash_passwd(login.passwd),
        start: login.start,
        channel: login.channel,
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: match env::consts::FAMILY {
            "mac" => "mac".to_string(),
            "win" => "win".to_string(),
            "unix" => "lin".to_string(),
            _ => "lin".to_string(),
        },
        platform_string: sys_info::os_release().unwrap_or_default(),
        platform_version: sys_info::os_release().unwrap_or_default(),
        mac: match get_mac_address() {
            Ok(Some(mac)) => format!("{}", mac),
            _ => format!("{}", 00000000000000000000000000000000),
        },
        id0: "unused".to_string(), // Provide a default value for id0
        agree_to_tos: login.agree_to_tos,
        read_critical: login.read_critical,
        viewer_digest: match hash_viewer_digest() {
            Ok(viewer_digest) => Some(viewer_digest),
            Err(_) => Some("unused".to_string()),
        },
        address_size: 64,                          // Set a default value if needed
        extended_errors: true,                     // Set a default value if needed
        last_exec_event: None,                     // Default to None
        last_exec_duration: 0,                     // Set a default value if needed
        skipoptional: None,                        // Default to None
        host_id: "".to_string(),                   // Set a default value if needed
        mfa_hash: "".to_string(),                  // Set a default value if needed
        token: "".to_string(),                     // Set a default value if needed
        options: SimulatorLoginOptions::default(), // Use default options
    }
}

/// md5 hashes the password
fn hash_passwd(passwd_raw: String) -> String {
    let mut hasher = md5::Md5::new();
    hasher.update(passwd_raw);
    format!("$1${:x}", hasher.finalize())
}

/// Creates the viewer digest, a fingerprint of the viewer executable
fn hash_viewer_digest() -> Result<String, Box<dyn Error>> {
    let path = env::args().next().ok_or("No argument found")?;
    let mut f = File::open(path)?;
    let mut byt = Vec::new();
    f.read_to_end(&mut byt)?;
    let hash = Md5::new().chain(&byt).finalize();
    Ok(format!("{:x}", hash))
}
