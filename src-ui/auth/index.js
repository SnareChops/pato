const { listen } = window.__TAURI__.event
const { Window } = window.__TAURI__.window
const { invoke } = window.__TAURI__.core

const clientId = 'arr27bhvnowepylzv8qs2tgaqc66yb'
const redirectUri = 'http://localhost:3000/auth/twitch'  // Use fixed port
const scopes = ['user:read:email']

export async function login(){
  // Set up event listeners for OAuth callbacks
  await listen('oauth-success', (event) => {
    console.log('✅ OAuth Success! Access token:', event.payload);
    // Store the token and update UI
    localStorage.setItem('twitch_access_token', event.payload);
    // You can emit a custom event or update UI state here
    document.dispatchEvent(new CustomEvent('twitch-auth-success', { 
      detail: { token: event.payload } 
    }));
  });

  await listen('oauth-error', (event) => {
    console.error('❌ OAuth Error:', event.payload);
    // Handle OAuth error
    document.dispatchEvent(new CustomEvent('twitch-auth-error', { 
      detail: { error: event.payload } 
    }));
  });

  const url = `https://id.twitch.tv/oauth2/authorize?client_id=${clientId}&redirect_uri=${encodeURIComponent(redirectUri)}&response_type=token&scope=${encodeURIComponent(scopes.join(' '))}`;
  await invoke('open_oauth_window', { url});
}

// Export function to get stored token
export function getAccessToken() {
  return localStorage.getItem('twitch_access_token');
}

// Export function to check if user is authenticated
export function isAuthenticated() {
  return !!getAccessToken();
}

console.log(window.__TAURI__.core)