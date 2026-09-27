//! `node sign`: produce the signature headers for a client-to-node request.

use anyhow::Result;
use net::auth;
use net::identity::Keypair;

use crate::cli::SignArgs;

pub fn run(args: &SignArgs) -> Result<()> {
    let keypair = Keypair::load_or_generate(&args.key_file)?;
    let signature = auth::sign_http_request(&keypair, &args.method, &args.path, auth::now_ms());

    println!("{} {}", auth::CLIENT_PUBLIC_KEY, signature.public_key);
    println!("{} {}", auth::CLIENT_TIMESTAMP, signature.timestamp_ms);
    println!("{} {}", auth::CLIENT_SIGNATURE, signature.signature);
    Ok(())
}
