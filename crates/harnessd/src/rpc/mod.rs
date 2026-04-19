mod approval;
mod auth;
mod chat;
mod config;
mod device;
mod errors;
mod events;
mod handshake;
mod mappers;
pub mod mcp;
mod session;
mod skill;

use crate::Daemon;
use harness_rpc::Router;
use std::sync::Arc;

pub fn build_router(d: &Arc<Daemon>) -> Router {
    let mut r = Router::new()
        .route("ping", handshake::ping())
        .route("status", handshake::status(d.clone()))
        .route("negotiate", handshake::negotiate())
        .route("v1.session.create", session::create(d.clone()))
        .route("v1.session.list", session::list(d.clone()))
        .route("v1.session.resume", session::resume(d.clone()))
        .route("v1.session.delete", session::delete(d.clone()))
        .route("v1.chat.send", chat::send(d.clone()))
        .route("v1.chat.cancel", chat::cancel(d.clone()))
        .route("v1.config.get", config::get(d.clone()))
        .route("v1.config.set", config::set(d.clone()))
        .route("v1.config.list", config::list(d.clone()))
        .route("v1.config.unset", config::unset(d.clone()))
        .route("v1.auth.credentials.add", auth::creds_add(d.clone()))
        .route("v1.auth.credentials.list", auth::creds_list(d.clone()))
        .route("v1.auth.credentials.delete", auth::creds_delete(d.clone()))
        .route("v1.auth.pair.new", auth::pair_new(d.clone()))
        .route("v1.device.list", device::list(d.clone()))
        .route("v1.device.revoke", device::revoke(d.clone()))
        .route("v1.skill.list", skill::list(d.clone()))
        .route("v1.skill.activate", skill::activate(d.clone()))
        .route("v1.approval.respond", approval::respond(d.clone()))
        .route("v1.mcp.add", mcp::add(d.clone()))
        .route("v1.mcp.list", mcp::list(d.clone()))
        .route("v1.mcp.remove", mcp::remove(d.clone()));
    if let Some(tls) = d.security.tls_fingerprint.clone() {
        r = r.route("v1.auth.fingerprint", handshake::fingerprint(tls));
    }
    r
}
