// YouTube OAuth authentication module
// Handles Google OAuth 2.0 flow and cookie management for yt-dlp

use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge, PkceCodeVerifier,
    RedirectUrl, Scope, TokenResponse, TokenUrl,
};
use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use chrono::{DateTime, Utc};

// YouTube OAuth endpoints
const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

// YouTube API scopes we need
const YOUTUBE_SCOPE: &str = "https://www.googleapis.com/auth/youtube.readonly";
const YOUTUBE_FORCE_SSL_SCOPE: &str = "https://www.googleapis.com/auth/youtube.force-ssl";

// Stored authentication data
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthData {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub user_email: Option<String>,
}

pub struct YouTubeAuth {
    client_id: String,
    client_secret: String,
    config_dir: PathBuf,
}

impl YouTubeAuth {
    pub fn new() -> Result<Self, String> {
        // Get config directory
        let config_dir = dirs::config_dir()
            .ok_or("Could not find config directory")?
            .join("youtube-music-player");

        // Create config directory if it doesn't exist
        fs::create_dir_all(&config_dir)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;

        // TODO: These should be your actual OAuth credentials from Google Cloud Console
        // For now, we'll use placeholders that the user needs to replace
        let client_id = std::env::var("YOUTUBE_CLIENT_ID")
            .unwrap_or_else(|_| "YOUR_CLIENT_ID_HERE".to_string());
        let client_secret = std::env::var("YOUTUBE_CLIENT_SECRET")
            .unwrap_or_else(|_| "YOUR_CLIENT_SECRET_HERE".to_string());

        Ok(YouTubeAuth {
            client_id,
            client_secret,
            config_dir,
        })
    }

    // Get path to auth data file
    fn auth_file_path(&self) -> PathBuf {
        self.config_dir.join("auth.json")
    }

    // Get path to cookies file for yt-dlp
    fn cookies_file_path(&self) -> PathBuf {
        self.config_dir.join("cookies.txt")
    }

    // Check if user is already authenticated
    pub fn is_authenticated(&self) -> bool {
        if let Ok(auth_data) = self.load_auth_data() {
            // Check if token is still valid
            Utc::now() < auth_data.expires_at
        } else {
            false
        }
    }

    // Load stored authentication data
    pub fn load_auth_data(&self) -> Result<AuthData, String> {
        let auth_path = self.auth_file_path();
        let data = fs::read_to_string(&auth_path)
            .map_err(|e| format!("Failed to read auth data: {}", e))?;

        serde_json::from_str(&data)
            .map_err(|e| format!("Failed to parse auth data: {}", e))
    }

    // Save authentication data
    fn save_auth_data(&self, auth_data: &AuthData) -> Result<(), String> {
        let auth_path = self.auth_file_path();
        let json = serde_json::to_string_pretty(auth_data)
            .map_err(|e| format!("Failed to serialize auth data: {}", e))?;

        fs::write(&auth_path, json)
            .map_err(|e| format!("Failed to write auth data: {}", e))?;

        Ok(())
    }

    // Start OAuth flow - returns authorization URL
    pub fn start_oauth_flow(&self) -> Result<(String, CsrfToken, PkceCodeVerifier), String> {
        let client = BasicClient::new(
            ClientId::new(self.client_id.clone()),
            Some(ClientSecret::new(self.client_secret.clone())),
            AuthUrl::new(GOOGLE_AUTH_URL.to_string())
                .map_err(|e| format!("Invalid auth URL: {}", e))?,
            Some(TokenUrl::new(GOOGLE_TOKEN_URL.to_string())
                .map_err(|e| format!("Invalid token URL: {}", e))?),
        )
        .set_redirect_uri(
            RedirectUrl::new("http://localhost:8080/callback".to_string())
                .map_err(|e| format!("Invalid redirect URL: {}", e))?,
        );

        // Generate PKCE challenge
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        // Generate authorization URL
        let (auth_url, csrf_token) = client
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new(YOUTUBE_SCOPE.to_string()))
            .add_scope(Scope::new(YOUTUBE_FORCE_SSL_SCOPE.to_string()))
            .set_pkce_challenge(pkce_challenge)
            .url();

        Ok((auth_url.to_string(), csrf_token, pkce_verifier))
    }

    // Complete OAuth flow with authorization code
    pub async fn complete_oauth_flow(
        &self,
        code: String,
        pkce_verifier: PkceCodeVerifier,
    ) -> Result<AuthData, String> {
        let client = BasicClient::new(
            ClientId::new(self.client_id.clone()),
            Some(ClientSecret::new(self.client_secret.clone())),
            AuthUrl::new(GOOGLE_AUTH_URL.to_string())
                .map_err(|e| format!("Invalid auth URL: {}", e))?,
            Some(TokenUrl::new(GOOGLE_TOKEN_URL.to_string())
                .map_err(|e| format!("Invalid token URL: {}", e))?),
        )
        .set_redirect_uri(
            RedirectUrl::new("http://localhost:8080/callback".to_string())
                .map_err(|e| format!("Invalid redirect URL: {}", e))?,
        );

        // Exchange authorization code for access token
        let token_result = client
            .exchange_code(AuthorizationCode::new(code))
            .set_pkce_verifier(pkce_verifier)
            .request_async(async_http_client)
            .await
            .map_err(|e| format!("Failed to exchange code: {}", e))?;

        // Calculate expiration time
        let expires_at = Utc::now() + chrono::Duration::seconds(
            token_result.expires_in()
                .map(|d| d.as_secs() as i64)
                .unwrap_or(3600)
        );

        let auth_data = AuthData {
            access_token: token_result.access_token().secret().clone(),
            refresh_token: token_result.refresh_token().map(|t| t.secret().clone()),
            expires_at,
            user_email: None,
        };

        // Save auth data
        self.save_auth_data(&auth_data)?;

        // Export cookies for yt-dlp
        self.export_cookies_for_ytdlp(&auth_data.access_token)?;

        Ok(auth_data)
    }

    // Export cookies in Netscape format for yt-dlp
    fn export_cookies_for_ytdlp(&self, access_token: &str) -> Result<(), String> {
        let cookies_path = self.cookies_file_path();

        // Create a simple cookie file with the access token
        // This is a simplified version - in production, you'd want to extract actual cookies
        let cookie_content = format!(
            "# Netscape HTTP Cookie File\n\
             .youtube.com\tTRUE\t/\tTRUE\t0\tLOGIN_INFO\t{}\n",
            access_token
        );

        fs::write(&cookies_path, cookie_content)
            .map_err(|e| format!("Failed to write cookies: {}", e))?;

        eprintln!("Cookies exported to: {:?}", cookies_path);
        Ok(())
    }

    // Get path to cookies file (for passing to yt-dlp)
    pub fn get_cookies_path(&self) -> Option<PathBuf> {
        let path = self.cookies_file_path();
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }

    // Logout - clear stored authentication
    pub fn logout(&self) -> Result<(), String> {
        let auth_path = self.auth_file_path();
        let cookies_path = self.cookies_file_path();

        if auth_path.exists() {
            fs::remove_file(&auth_path)
                .map_err(|e| format!("Failed to remove auth file: {}", e))?;
        }

        if cookies_path.exists() {
            fs::remove_file(&cookies_path)
                .map_err(|e| format!("Failed to remove cookies file: {}", e))?;
        }

        Ok(())
    }
}
