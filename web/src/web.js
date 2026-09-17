// Web-only chrome around the shared dashboard: the header download buttons
// and, on the PWA build, the install flow. The desktop app never loads this.
(function () {
  'use strict';

  const RELEASE_ZIP =
    'https://github.com/AndreewCore/Dota-Stats-CLI-APP/releases/latest/download/dota-stats-windows-x86_64.zip';
  // Production domain of the separate, installable PWA project on Vercel.
  const PWA_URL = 'https://dota-stats-cap-pwa.vercel.app/';

  const target = document.documentElement.dataset.target; // 'site' | 'pwa'
  const box = document.getElementById('getApp');
  const appBtn = document.getElementById('getPwa');
  const winBtn = document.getElementById('getWin');
  if (!box || !target) return;

  winBtn.href = RELEASE_ZIP;
  box.hidden = false;

  if (target === 'site') {
    // The site itself stays non-installable; installing only happens on the
    // PWA's own origin, which this button leads to.
    appBtn.href = PWA_URL;
    return;
  }

  /** True when already running as an installed app, where "Install" is moot. */
  const standalone = () =>
    window.matchMedia('(display-mode: standalone)').matches || navigator.standalone === true;

  // Chrome/Edge/Android: hold the prompt so installing only ever starts from
  // the button, never from the browser's automatic mini-infobar.
  let deferred = null;
  window.addEventListener('beforeinstallprompt', (e) => { e.preventDefault(); deferred = e; });
  window.addEventListener('appinstalled', () => { appBtn.hidden = true; });

  appBtn.textContent = 'Install app';
  appBtn.removeAttribute('href');
  appBtn.hidden = standalone();
  appBtn.addEventListener('click', async (e) => {
    e.preventDefault();
    if (deferred) {
      deferred.prompt();
      await deferred.userChoice;
      deferred = null;
      return;
    }
    showInstallHelp();
  });

  /** Explain the manual route on browsers with no programmatic install (iOS, Firefox). */
  function showInstallHelp() {
    const ios = /iphone|ipad|ipod/i.test(navigator.userAgent)
      || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);
    const steps = ios
      ? 'In Safari, tap <b>Share</b> and then <b>Add to Home Screen</b>.'
      : 'Open the browser menu and choose <b>Install app</b> (Chrome, Edge, Samsung Internet). '
        + 'Firefox on desktop cannot install web apps — use one of those browsers instead.';
    // pushView/mHead are app.js globals: reusing the dashboard's modal keeps
    // one overlay, one close button and one Escape handler.
    pushView(async () => mHead(
      '<div class="big" style="display:flex;align-items:center;justify-content:center;font-size:22px">⬇</div>',
      'Install Dota 2 Stats', 'Add the dashboard to your device')
      + `<div class="mbody"><p>${steps}</p></div>`);
  }

  // Installing is what this build exists for, so it claims the first view from
  // the profile editor app.js would otherwise open with no profiles saved.
  // Closing it lands on the empty dashboard and its own "+ Add a Dota ID"
  // button, so nothing is lost. An installed app runs standalone and skips it.
  // Set here rather than pushed on load because app.js reads it during its own
  // startup: this script is the previous tag, so it always wins the race.
  if (!standalone()) window.startupView = showInstallHelp;

  if ('serviceWorker' in navigator) {
    navigator.serviceWorker.register('sw.js').catch((err) => console.error('service worker:', err));
  }
})();
