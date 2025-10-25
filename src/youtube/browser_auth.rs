// Browser-based authentication
// Detects YouTube accounts from Chrome/Firefox cookies

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserAccount {
    pub browser: String,        // "chrome", "firefox", etc
    pub profile: String,         // "Default", "Profile 1", etc
    pub email: Option<String>,   // User's email if we can extract it
    pub display_name: String,    // What to show in UI
}

pub struct BrowserAuth {
    config_dir: PathBuf,
}

impl BrowserAuth {
    pub fn new() -> Result<Self, String> {
        let config_dir = dirs::config_dir()
            .ok_or("Could not find config directory")?
            .join("youtube-music-player");

        std::fs::create_dir_all(&config_dir)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;

        Ok(BrowserAuth { config_dir })
    }

    // Detect available YouTube accounts from installed browsers
    pub fn detect_accounts(&self) -> Vec<BrowserAccount> {
        let mut accounts = Vec::new();

        // Try Zen Browser (Firefox fork) first
        accounts.extend(self.detect_zen_accounts());

        // Try Chrome
        accounts.extend(self.detect_chrome_accounts());

        // Try Firefox
        accounts.extend(self.detect_firefox_accounts());

        accounts
    }

    fn detect_chrome_accounts(&self) -> Vec<BrowserAccount> {
        let mut accounts = Vec::new();

        // Chrome config locations by OS
        let chrome_base = if cfg!(target_os = "linux") {
            dirs::config_dir().map(|d| d.join("google-chrome"))
        } else if cfg!(target_os = "macos") {
            dirs::home_dir().map(|d| d.join("Library/Application Support/Google/Chrome"))
        } else if cfg!(target_os = "windows") {
            dirs::data_local_dir().map(|d| d.join("Google/Chrome/User Data"))
        } else {
            None
        };

        if let Some(chrome_dir) = chrome_base {
            if chrome_dir.exists() {
                // Check Default profile
                let default_profile = chrome_dir.join("Default");
                if default_profile.exists() {
                    accounts.push(BrowserAccount {
                        browser: "chrome".to_string(),
                        profile: "Default".to_string(),
                        email: None,
                        display_name: "Chrome - Default Profile".to_string(),
                    });
                }

                // Check other profiles (Profile 1, Profile 2, etc)
                for i in 1..10 {
                    let profile_dir = chrome_dir.join(format!("Profile {}", i));
                    if profile_dir.exists() {
                        accounts.push(BrowserAccount {
                            browser: "chrome".to_string(),
                            profile: format!("Profile {}", i),
                            email: None,
                            display_name: format!("Chrome - Profile {}", i),
                        });
                    }
                }
            }
        }

        accounts
    }

    fn detect_zen_accounts(&self) -> Vec<BrowserAccount> {
        let mut accounts = Vec::new();

        // Zen Browser config locations (Firefox fork)
        let zen_base = if cfg!(target_os = "linux") {
            dirs::home_dir().map(|d| d.join(".zen"))
        } else if cfg!(target_os = "macos") {
            dirs::home_dir().map(|d| d.join("Library/Application Support/Zen"))
        } else if cfg!(target_os = "windows") {
            dirs::data_dir().map(|d| d.join("Zen"))
        } else {
            None
        };

        if let Some(zen_dir) = zen_base {
            if zen_dir.exists() {
                // Zen uses Firefox-style profile structure
                if let Ok(entries) = std::fs::read_dir(&zen_dir) {
                    for entry in entries.flatten() {
                        if entry.path().is_dir() {
                            let profile_name = entry.file_name().to_string_lossy().to_string();
                            // Skip special directories
                            if !profile_name.starts_with('.') && profile_name != "firefox-mpris" && profile_name != "Profile Groups" {
                                // Check if this profile has cookies
                                let cookies_path = entry.path().join("cookies.sqlite");
                                if cookies_path.exists() {
                                    accounts.push(BrowserAccount {
                                        browser: "zen".to_string(),
                                        profile: profile_name.clone(),
                                        email: None,
                                        display_name: format!("Zen Browser - {}", profile_name),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        accounts
    }

    fn detect_firefox_accounts(&self) -> Vec<BrowserAccount> {
        let mut accounts = Vec::new();

        // Firefox config locations
        let firefox_base = if cfg!(target_os = "linux") {
            dirs::home_dir().map(|d| d.join(".mozilla/firefox"))
        } else if cfg!(target_os = "macos") {
            dirs::home_dir().map(|d| d.join("Library/Application Support/Firefox/Profiles"))
        } else if cfg!(target_os = "windows") {
            dirs::data_dir().map(|d| d.join("Mozilla/Firefox/Profiles"))
        } else {
            None
        };

        if let Some(firefox_dir) = firefox_base {
            if firefox_dir.exists() {
                // Firefox uses random profile names, just detect any
                if let Ok(entries) = std::fs::read_dir(&firefox_dir) {
                    for entry in entries.flatten() {
                        if entry.path().is_dir() {
                            let profile_name = entry.file_name().to_string_lossy().to_string();
                            // Skip special directories
                            if !profile_name.starts_with('.') {
                                accounts.push(BrowserAccount {
                                    browser: "firefox".to_string(),
                                    profile: profile_name.clone(),
                                    email: None,
                                    display_name: format!("Firefox - {}", profile_name),
                                });
                            }
                        }
                    }
                }
            }
        }

        accounts
    }

    // Save selected account
    pub fn save_selected_account(&self, account: &BrowserAccount) -> Result<(), String> {
        let config_path = self.config_dir.join("selected_account.json");
        let json = serde_json::to_string_pretty(account)
            .map_err(|e| format!("Failed to serialize account: {}", e))?;

        std::fs::write(&config_path, json)
            .map_err(|e| format!("Failed to write account: {}", e))?;

        Ok(())
    }

    // Load previously selected account
    pub fn load_selected_account(&self) -> Option<BrowserAccount> {
        let config_path = self.config_dir.join("selected_account.json");
        if !config_path.exists() {
            return None;
        }

        let data = std::fs::read_to_string(&config_path).ok()?;
        serde_json::from_str(&data).ok()
    }

    // Get yt-dlp cookie arguments
    // Returns (use_cookies_from_browser: bool, arg: String)
    pub fn get_cookie_arg(&self, account: &BrowserAccount) -> (bool, String) {
        match account.browser.as_str() {
            "chrome" => {
                let arg = if account.profile == "Default" {
                    "chrome".to_string()
                } else {
                    format!("chrome:{}", account.profile)
                };
                (true, arg)
            }
            "firefox" => {
                (true, format!("firefox:{}", account.profile))
            }
            "zen" => {
                // Zen Browser: treat as Firefox since it's a Firefox fork
                // yt-dlp can extract cookies from Firefox-based browsers
                // Pass the full profile path
                if cfg!(target_os = "linux") {
                    if let Some(home) = dirs::home_dir() {
                        let profile_path = home.join(".zen").join(&account.profile);
                        // Use format: firefox:/path/to/profile
                        return (true, format!("firefox:{}", profile_path.display()));
                    }
                } else if cfg!(target_os = "macos") {
                    if let Some(home) = dirs::home_dir() {
                        let profile_path = home.join("Library/Application Support/Zen").join(&account.profile);
                        return (true, format!("firefox:{}", profile_path.display()));
                    }
                } else if cfg!(target_os = "windows") {
                    if let Some(data_dir) = dirs::data_dir() {
                        let profile_path = data_dir.join("Zen").join(&account.profile);
                        return (true, format!("firefox:{}", profile_path.display()));
                    }
                }
                // Fallback
                (true, format!("firefox:{}", account.profile))
            }
            _ => (true, "chrome".to_string()), // fallback
        }
    }

    // Check if user has selected an account
    pub fn is_authenticated(&self) -> bool {
        self.load_selected_account().is_some()
    }
}
