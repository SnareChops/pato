// Define the button click function globally
async function clickButton() {
  // @ts-ignore
  const { invoke } = window.__TAURI__.core;
  await invoke('handle_button_click');
}

/**
 * @param {string} message
 */
function showResult(message) {
  const element = document.getElementById('result');
  if (element) {
    if (message.includes('Plugin returned:')) {
      element.innerHTML = '<p style="color: green;">✅ ' + message + '</p>';
    } else if (message.includes('failed:')) {
      element.innerHTML = '<p style="color: red;">❌ ' + message + '</p>';
    } else {
      element.innerHTML = '<p>' + message + '</p>';
    }
  }
}

// Wait for DOM and Tauri to be ready
document.addEventListener('DOMContentLoaded', async () => {
  // @ts-ignore
  const { listen } = window.__TAURI__.event;
  
  // Listen for events from Rust
  await listen('button-clicked', (/** @type {{ payload: string }} */ event) => {
    showResult(event.payload);
  });
});

// Make functions available globally
window.clickButton = clickButton;
window.showResult = showResult;