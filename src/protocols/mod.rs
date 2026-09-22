pub mod clash_meta;
pub mod general;
pub mod loon;
pub mod quantumult_x;
pub mod shadowrocket;
pub mod singbox;
pub mod surge;
pub mod types;

pub use clash_meta::ClashMetaProtocol;
pub use general::GeneralProtocol;
pub use loon::LoonProtocol;
pub use quantumult_x::QuantumultXProtocol;
pub use shadowrocket::ShadowrocketProtocol;
pub use singbox::SingBoxProtocol;
pub use surge::SurgeProtocol;
pub use types::{ProxyContext, SubscriptionOutput};

use crate::entities::server::Model as ServerModel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientType {
    ClashMeta,
    Clash,
    SingBox,
    Surge,
    Shadowrocket,
    QuantumultX,
    Loon,
    General,
}

/// Detects the target proxy client type based on `flag` query parameter or `User-Agent` header.
pub fn detect_client(flag: Option<&str>, user_agent: Option<&str>) -> ClientType {
    // 1. Check flag query parameter first
    if let Some(f) = flag {
        let fl = f.to_lowercase();
        match fl.as_str() {
            "meta" | "verge" | "clashmeta" | "clash-verge" | "flclash" | "nekobox" | "mihomo" => {
                return ClientType::ClashMeta
            }
            "clash" => return ClientType::Clash,
            "sing-box" | "singbox" | "hiddify" | "sfm" => return ClientType::SingBox,
            "surge" => return ClientType::Surge,
            "shadowrocket" => return ClientType::Shadowrocket,
            "quantumult x" | "quantumult%20x" | "quantumult-x" | "quanx" => {
                return ClientType::QuantumultX
            }
            "loon" => return ClientType::Loon,
            "general" | "v2rayn" | "v2rayng" | "passwall" | "ssrplus" => {
                return ClientType::General
            }
            _ => {}
        }
    }

    // 2. Fall back to User-Agent header inspection
    if let Some(ua) = user_agent {
        let ual = ua.to_lowercase();
        if ual.contains("mihomo")
            || ual.contains("meta")
            || ual.contains("clash-verge")
            || ual.contains("flclash")
            || ual.contains("nekobox")
        {
            return ClientType::ClashMeta;
        }
        if ual.contains("clash") {
            return ClientType::Clash;
        }
        if ual.contains("sing-box") || ual.contains("hiddify") {
            return ClientType::SingBox;
        }
        if ual.contains("surge") {
            return ClientType::Surge;
        }
        if ual.contains("shadowrocket") {
            return ClientType::Shadowrocket;
        }
        if ual.contains("quantumult x") || ual.contains("quantumult") {
            return ClientType::QuantumultX;
        }
        if ual.contains("loon") {
            return ClientType::Loon;
        }
    }

    ClientType::General
}

/// Dispatches subscription generation to the corresponding client protocol handler.
pub fn generate_subscription(
    client_type: ClientType,
    context: &ProxyContext,
    servers: &[ServerModel],
) -> SubscriptionOutput {
    match client_type {
        ClientType::ClashMeta | ClientType::Clash => ClashMetaProtocol::handle(context, servers),
        ClientType::SingBox => SingBoxProtocol::handle(context, servers),
        ClientType::Surge => SurgeProtocol::handle(context, servers),
        ClientType::Shadowrocket => ShadowrocketProtocol::handle(context, servers),
        ClientType::QuantumultX => QuantumultXProtocol::handle(context, servers),
        ClientType::Loon => LoonProtocol::handle(context, servers),
        ClientType::General => GeneralProtocol::handle(context, servers),
    }
}
