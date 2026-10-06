//! Commande `nestord onboard` : fournit le jeton d'authentification a saisir
//! dans l'app mobile (ou le front web) pour se connecter a `/ws`.
//!
//! Le jeton est genere puis affiche une seule fois ; seule son empreinte SHA-256
//! est ecrite dans `~/.config/nestord/auth_token` (droits 0600) et relue par le
//! daemon au demarrage (cf. `Config::load`) : lire ce fichier ne donne pas le jeton.
//! `NESTORD_AUTH_TOKEN` et `auth_token` du TOML restent prioritaires.

use std::io::Read;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::config::{Config, token_file_path};

/// Genere 24 octets aleatoires (48 caracteres hexadecimaux) depuis le noyau.
fn generate_token() -> Result<String> {
    let mut bytes = [0u8; 24];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .context("lecture de /dev/urandom")?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Ecrit l'empreinte du jeton avec des droits restreints au proprietaire.
fn persist_token(path: &PathBuf, token: &str) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("ecriture de {}", path.display()))?;
    writeln!(file, "{token}")?;
    Ok(())
}

/// Nom DNS MagicDNS de cette machine, si Tailscale est disponible.
fn tailscale_host() -> Option<String> {
    let out = std::process::Command::new("tailscale")
        .args(["status", "--json"])
        .output()
        .ok()?;
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let name = json.get("Self")?.get("DNSName")?.as_str()?;
    Some(name.trim_end_matches('.').to_string())
}

/// Point d'entree : `nestord onboard [--rotate] [--url <wss://hote[:port]/ws>]`.
pub fn run(args: &[String]) -> Result<()> {
    let mut rotate = false;
    let mut passkey = false;
    let mut ui_url: Option<String> = None;
    let mut base_url: Option<String> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--rotate" => rotate = true,
            "--passkey" => passkey = true,
            "--ui" => ui_url = iter.next().cloned(),
            "--url" => base_url = iter.next().cloned(),
            other => anyhow::bail!("option inconnue : {other} (attendu : --passkey, --ui <url>, --rotate, --url <url>)"),
        }
    }

    if passkey {
        return onboard_passkey(ui_url);
    }

    let path = token_file_path();
    // nestord ne conserve que l'empreinte du jeton : un jeton deja cree ne peut plus etre affiche.
    if Config::load().auth.is_some() && !rotate {
        println!("Un jeton d'acces est deja configure. nestord n'en garde que l'empreinte : il ne peut");
        println!("pas etre reaffiche. Pour en creer un nouveau (l'ancien cesse de fonctionner) :");
        println!("\n  nestord onboard --rotate\n");
        return Ok(());
    }
    if std::env::var("NESTORD_AUTH_TOKEN").is_ok_and(|t| !t.trim().is_empty()) {
        eprintln!("Attention : NESTORD_AUTH_TOKEN est defini et reste prioritaire sur le fichier.");
    }
    let token = generate_token()?;
    persist_token(&path, &crate::auth::TokenHash::of(&token).to_hex())?;
    let source = "affiche une seule fois, notez-le";

    let url = base_url
        .or_else(|| tailscale_host().map(|h| format!("wss://{h}:8443/ws")))
        .unwrap_or_else(|| "wss://<hote>:8443/ws".to_string());
    let separator = if url.contains('?') { '&' } else { '?' };

    println!("Jeton d'onboarding ({source}) :\n\n  {token}\n");
    println!("URL complete a coller dans l'app mobile :\n\n  {url}{separator}token={token}\n");
    println!("Empreinte enregistree dans : {}", path.display());
    println!("Redemarrez nestord pour que le jeton soit exige sur /ws.");
    println!("`nestord onboard --rotate` revoque ce jeton et en genere un nouveau.");
    Ok(())
}

/// `nestord onboard --passkey [--ui <url>]` : lien d'enrolement d'une passkey pour l'interface web.
fn onboard_passkey(ui_url: Option<String>) -> Result<()> {
    let ui = ui_url.unwrap_or_else(|| Config::load().ui_url);
    let ui = ui.trim_end_matches('/');
    let code = crate::passkey::create_enroll_code()?;
    let minutes = crate::passkey::ENROLL_TTL.as_secs() / 60;

    println!("Lien d'enrolement d'une passkey (valable {minutes} minutes, utilisable une seule fois) :\n");
    println!("  {ui}/?enroll={code}\n");
    println!("Ouvrez-le dans le navigateur a equiper, puis suivez l'invite de creation de la passkey.");
    println!("Des qu'une passkey est enregistree, l'interface doit s'authentifier pour se connecter.");
    println!("Une passkey est liee a un nom de domaine : `localhost` ou une adresse en HTTPS, pas une adresse IP.");
    println!("Autre adresse d'interface : `nestord onboard --passkey --ui https://mon-adresse`.");
    Ok(())
}
