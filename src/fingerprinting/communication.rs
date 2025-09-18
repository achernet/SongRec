use serde_json::{json, Value};
use reqwest::header::HeaderMap;
use std::time::SystemTime;
use std::env;
use std::error::Error;
use std::time::Duration;
use rand::seq::SliceRandom;
use uuid::Uuid;

use crate::fingerprinting::signature_format::DecodedSignature;
use crate::fingerprinting::user_agent::USER_AGENTS;

pub fn recognize_song_from_signature(signature: &DecodedSignature) -> Result<Value, Box<dyn Error>>  {
    
    let timestamp_ms = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_millis();
    
    let post_data = json!({
        "geolocation": {
            "altitude": 300,
            "latitude": 45,
            "longitude": 2
        },
        "signature": {
            "samplems": (signature.number_samples as f32 / signature.sample_rate_hz as f32 * 1000.) as u32,
            "timestamp": timestamp_ms as u32,
            "uri": signature.encode_to_uri()?
        },
        "timestamp": timestamp_ms as u32,
        "timezone": "Europe/Paris"
    });

    let uuid_1 = Uuid::new_v4().to_hyphenated().to_string().to_uppercase();
    let uuid_2 = Uuid::new_v4().to_hyphenated().to_string();

    let url = format!("https://amp.shazam.com/discovery/v5/en/US/android/-/tag/{}/{}", uuid_1, uuid_2);

    let mut headers = HeaderMap::new();
    
    headers.insert("User-Agent", USER_AGENTS.choose(&mut rand::thread_rng()).unwrap().parse()?);
    headers.insert("Content-Language", "en_US".parse()?);

    let client = reqwest_client()?;
    let max_retries = 10;
    let timeout = Duration::from_secs(15);
    for attempt in 0..=max_retries {
        let response = client
            .post(&url)
            .timeout(timeout)
            .query(&[
                ("sync", "true"),
                ("webv3", "true"),
                ("sampling", "true"),
                ("connected", ""),
                ("shazamapiversion", "v3"),
                ("sharehub", "true"),
                ("video", "v3"),
            ])
            .headers(headers.clone())
            .json(&post_data)
            .send();

        match response {
            Ok(resp) => {
                let status = resp.status();
                if status.as_u16() == 429 || status.as_u16() == 529 {
                    if attempt < max_retries {
                        let backoff = Duration::from_millis((1.5f64.powi(attempt) * 1000f64) as u64);
                        eprintln!(
                            "Rate limited ({}), retrying in {}s...",
                            status,
                            backoff.as_secs()
                        );
                        std::thread::sleep(backoff);
                        continue;
                    } else {
                        return Err(format!(
                            "Max retries exceeded due to rate limiting ({}).",
                            status
                        )
                            .into());
                    }
                } else if status.is_success() {
                    return Ok(resp.json()?);
                } else {
                    return Err(format!("Unexpected HTTP status: {}", status).into());
                }
            }
            Err(err) => {
                return Err(Box::new(err));
            }
        }
    }

    Err("Unreachable: retry loop exhausted".into())
}

pub fn obtain_raw_cover_image(url: &str) -> Result<Vec<u8>, Box<dyn Error>> {

    let mut headers = HeaderMap::new();
    
    headers.insert("User-Agent", USER_AGENTS.choose(&mut rand::thread_rng()).unwrap().parse()?);
    headers.insert("Content-Language", "en_US".parse()?);

    let client = reqwest_client()?;
    let response = client.get(url)
        .timeout(Duration::from_secs(20))
        .headers(headers)
        .send()?;
    
    Ok(response.bytes()?.as_ref().to_vec())

}

fn reqwest_client() -> Result<reqwest::blocking::Client, Box<dyn Error>> {
    let mut client = reqwest::blocking::Client::builder();
    if let Ok(proxy) = env::var("https_proxy") {
        client = client.proxy(reqwest::Proxy::https(&proxy)?);
    } else if let Ok(proxy) = env::var("HTTPS_PROXY") {
        client = client.proxy(reqwest::Proxy::https(&proxy)?);
    };
    Ok(client.build()?)
}
