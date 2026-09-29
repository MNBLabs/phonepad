/*
 * DynShift privacy runtime, shared by dynshift.com, layr.dynshift.com and phonepad.dynshift.com.
 * Each site ships an identical copy; change all three together.
 *
 * Load it synchronously in <head>, before the AdSense loader and before any Google tag:
 *   <script src="/privacy.js" data-ga="G-XXXX" data-policy="/privacy/" data-cookies="/cookies/"></script>
 *
 * What it does, in order:
 *  1. Consent Mode v2 defaults, before any Google tag: analytics denied everywhere; advertising denied
 *     in the EEA, the UK and Switzerland until Google's consent message answers; ad personalisation
 *     and ad user data denied everywhere when the browser sends Global Privacy Control.
 *  2. Analytics (GA4) loads only after consent: Google's message where it applies, our own two-button
 *     banner elsewhere. Never under GPC. Nothing is fetched from Google Analytics before that.
 *  3. Ad slots (<aside class="ad" data-ad-slot-lazy>): pushed only when near the viewport, restricted
 *     data processing under GPC, and never on legal pages.
 *  4. The footer "Privacy settings" control opens one small panel; where US state regulations apply,
 *     the footer's "Do Not Sell or Share My Personal Information" link opens Google's opt-out dialog.
 */
(function () {
  "use strict";
  var w = window;
  var d = document;
  var me = d.currentScript;
  var cfg = {
    ga: (me && me.getAttribute("data-ga")) || "",
    policy: (me && me.getAttribute("data-policy")) || "/privacy/",
    cookies: (me && me.getAttribute("data-cookies")) || "/cookies/",
  };
  var KEY = "dynshift-consent";
  var VERSION = 1;
  var MAX_AGE = 365 * 24 * 3600 * 1000;
  var LEGAL = /^\/(privacy|cookies|terms|contact)(\/|$)/;
  var GDPR_REGIONS = [
    "AT", "BE", "BG", "HR", "CY", "CZ", "DK", "EE", "FI", "FR", "DE", "GR", "HU", "IE", "IT", "LV",
    "LT", "LU", "MT", "NL", "PL", "PT", "RO", "SK", "SI", "ES", "SE", "IS", "LI", "NO", "GB", "CH",
  ];

  var gpc = navigator.globalPrivacyControl === true;

  // 1. Consent Mode v2 defaults. The region-specific default wins for visitors in those regions.
  w.dataLayer = w.dataLayer || [];
  function gtag() {
    w.dataLayer.push(arguments);
  }
  w.gtag = w.gtag || gtag;
  gtag("consent", "default", {
    ad_storage: "granted",
    ad_user_data: gpc ? "denied" : "granted",
    ad_personalization: gpc ? "denied" : "granted",
    analytics_storage: "denied",
    functionality_storage: "granted",
    security_storage: "granted",
  });
  gtag("consent", "default", {
    ad_storage: "denied",
    ad_user_data: "denied",
    ad_personalization: "denied",
    analytics_storage: "denied",
    region: GDPR_REGIONS,
    wait_for_update: 500,
  });
  gtag("set", "ads_data_redaction", true);
  if (gpc) d.documentElement.setAttribute("data-gpc", "");

  // Our own "Do Not Sell or Share" link replaces Google's floating one (it lives in the footer).
  w.googlefc = w.googlefc || {};
  w.googlefc.callbackQueue = w.googlefc.callbackQueue || [];
  w.googlefc.usstatesoptout = w.googlefc.usstatesoptout || {};
  w.googlefc.usstatesoptout.overrideDnsLink = true;

  function read() {
    try {
      var v = JSON.parse(localStorage.getItem(KEY) || "null");
      if (!v || v.v !== VERSION || !v.at || Date.now() - Date.parse(v.at) > MAX_AGE) return null;
      return v;
    } catch (e) {
      return null;
    }
  }
  function write(analytics) {
    try {
      localStorage.setItem(KEY, JSON.stringify({ v: VERSION, analytics: analytics, at: new Date().toISOString() }));
    } catch (e) {
      // Storage blocked: the choice lasts for this page only.
    }
  }

  // 2. Analytics.
  var gaLoaded = false;
  function loadAnalytics() {
    if (!cfg.ga || gaLoaded || gpc) return;
    gaLoaded = true;
    gtag("js", new Date());
    gtag("config", cfg.ga);
    var s = d.createElement("script");
    s.async = true;
    s.src = "https://www.googletagmanager.com/gtag/js?id=" + encodeURIComponent(cfg.ga);
    d.head.appendChild(s);
  }
  function grantAnalytics() {
    gtag("consent", "update", { analytics_storage: "granted" });
    loadAnalytics();
  }
  function clearAnalyticsCookies() {
    var parts = location.hostname.split(".");
    var domains = ["", location.hostname, "." + parts.slice(-2).join(".")];
    d.cookie.split(";").forEach(function (c) {
      var name = c.split("=")[0].trim();
      if (name !== "_ga" && name.indexOf("_ga_") !== 0) return;
      domains.forEach(function (dom) {
        d.cookie = name + "=; Max-Age=0; path=/" + (dom ? "; domain=" + dom : "");
      });
    });
  }

  var gdprApplies = null; // null until Google's consent platform has said, or has not loaded in time
  var usStatus = 0;

  function whenTcf(cb) {
    if (typeof w.__tcfapi !== "function") return cb(null);
    w.__tcfapi("addEventListener", 2, function (tc, ok) {
      if (!ok || !tc) return;
      if (tc.eventStatus === "tcloaded" || tc.eventStatus === "useractioncomplete" || tc.gdprApplies === false) cb(tc);
    });
  }

  function decideAnalytics() {
    if (!cfg.ga || gpc) return;
    if (gdprApplies) {
      // Google's message asks here. Load only once it records consent for storage (purpose 1) and for
      // Google (vendor 755); Consent Mode, updated by the same message, then governs the cookies.
      whenTcf(function (tc) {
        if (tc && tc.purpose && tc.purpose.consents[1] && tc.vendor && tc.vendor.consents[755]) loadAnalytics();
      });
      return;
    }
    var choice = read();
    if (choice && choice.analytics === "granted") grantAnalytics();
    else if (!choice) showBanner();
  }

  // 3. Ad slots.
  function mountAds() {
    var slots = d.querySelectorAll("aside.ad[data-ad-slot-lazy]");
    if (!slots.length) return;
    if (LEGAL.test(location.pathname)) {
      for (var i = 0; i < slots.length; i++) slots[i].remove();
      return;
    }
    function push(slot) {
      if (slot.getAttribute("data-pushed")) return;
      slot.setAttribute("data-pushed", "1");
      var ins = slot.querySelector("ins.adsbygoogle");
      if (!ins) return;
      if (gpc) ins.setAttribute("data-restrict-data-processing", "1");
      try {
        (w.adsbygoogle = w.adsbygoogle || []).push({});
      } catch (e) {
        // Blocked by the reader's browser: the slot stays collapsed.
      }
    }
    if (!("IntersectionObserver" in w)) {
      for (var j = 0; j < slots.length; j++) push(slots[j]);
      return;
    }
    var io = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (e) {
          if (!e.isIntersecting) return;
          io.unobserve(e.target);
          push(e.target);
        });
      },
      { rootMargin: "600px 0px" },
    );
    for (var k = 0; k < slots.length; k++) io.observe(slots[k]);
  }
  // Slots added later (single-page navigation) call this.
  w.dynshiftAds = { mount: mountAds };

  // Our banner and panel. Allow and Reject are the same size, style and weight. Colours come from the
  // site: --ds-bg, --ds-ink, --ds-muted, --ds-line, --ds-radius (each site maps its own tokens).
  var styled = false;
  function css() {
    if (styled) return;
    styled = true;
    var s = d.createElement("style");
    s.textContent =
      ".ds-consent,.ds-panel{background:var(--ds-bg,#fff);color:var(--ds-ink,#111);border:1px solid var(--ds-line,#ddd);border-radius:var(--ds-radius,12px);box-shadow:0 8px 32px rgba(0,0,0,.18);font:inherit;font-size:.92rem;line-height:1.5}" +
      ".ds-consent{position:fixed;z-index:1000;left:16px;right:16px;bottom:16px;max-width:520px;margin-inline:auto;padding:16px 18px}" +
      ".ds-consent p,.ds-panel p{margin:0 0 12px;color:var(--ds-muted,#444)}" +
      ".ds-consent a,.ds-panel a{color:inherit;text-decoration:underline}" +
      ".ds-row{display:flex;flex-wrap:wrap;gap:8px}" +
      ".ds-btn{flex:1 1 0;min-width:120px;min-height:40px;padding:8px 14px;border-radius:calc(var(--ds-radius,12px) - 4px);border:1px solid var(--ds-ink,#111);background:transparent;color:var(--ds-ink,#111);font:inherit;font-weight:600;cursor:pointer}" +
      ".ds-btn:hover{background:color-mix(in srgb,var(--ds-ink,#111) 8%,transparent)}" +
      ".ds-btn:focus-visible{outline:2px solid var(--ds-ink,#111);outline-offset:2px}" +
      ".ds-privacy{position:fixed;inset:0;z-index:1001;display:grid;place-items:center;padding:16px;background:rgba(0,0,0,.45)}" +
      ".ds-panel{width:100%;max-width:460px;max-height:calc(100vh - 32px);overflow:auto;padding:20px 22px}" +
      ".ds-panel h2{margin:0 0 12px;font-size:1.15rem;color:var(--ds-ink,#111)}" +
      ".ds-panel h3{margin:16px 0 6px;font-size:.95rem;color:var(--ds-ink,#111)}" +
      ".ds-note{padding:10px 12px;border:1px solid var(--ds-line,#ddd);border-radius:8px}" +
      ".ds-links{margin-top:16px!important}";
    d.head.appendChild(s);
  }
  function el(tag, attrs, html) {
    var n = d.createElement(tag);
    for (var a in attrs) n.setAttribute(a, attrs[a]);
    if (html != null) n.innerHTML = html;
    return n;
  }
  function showBanner() {
    if (d.getElementById("ds-consent")) return;
    css();
    var box = el("div", { id: "ds-consent", class: "ds-consent", role: "region", "aria-label": "Analytics choice" });
    box.innerHTML =
      '<p>May this site count visits with Google Analytics? It shows which pages help. Nothing is loaded unless you allow it, and the site works the same either way. <a href="' +
      cfg.policy +
      '">Privacy</a></p><div class="ds-row"><button type="button" class="ds-btn" data-v="granted">Allow</button><button type="button" class="ds-btn" data-v="denied">Reject</button></div>';
    box.addEventListener("click", function (e) {
      var v = e.target && e.target.getAttribute && e.target.getAttribute("data-v");
      if (!v) return;
      write(v);
      box.remove();
      if (v === "granted") grantAnalytics();
    });
    d.body.appendChild(box);
  }

  function openPanel() {
    var old = d.getElementById("ds-privacy");
    if (old) old.remove();
    css();
    var choice = read();
    var html = '<div class="ds-panel" role="dialog" aria-modal="true" aria-labelledby="ds-privacy-title"><h2 id="ds-privacy-title">Privacy settings</h2>';
    if (gpc) html += '<p class="ds-note">Your browser sent Global Privacy Control. It is honoured here: no analytics, and no personalised advertising.</p>';
    if (cfg.ga && !gpc) {
      if (gdprApplies) {
        html += "<h3>Analytics</h3><p>Asked together with advertising, in the consent choices below.</p>";
      } else {
        var on = choice && choice.analytics === "granted";
        html +=
          "<h3>Analytics</h3><p>Google Analytics is currently <strong>" +
          (on ? "allowed" : "off") +
          '</strong>.</p><div class="ds-row"><button type="button" class="ds-btn" data-a="granted">Allow</button><button type="button" class="ds-btn" data-a="denied">Reject</button></div>';
      }
    }
    html += "<h3>Advertising</h3>";
    if (gdprApplies && w.googlefc && w.googlefc.showRevocationMessage) {
      html += '<p>Ads are supplied by Google. Review or change what you agreed to.</p><div class="ds-row"><button type="button" class="ds-btn" data-g="1">Consent choices</button></div>';
    } else if (usStatus === 2 || usStatus === 3) {
      html +=
        "<p>Ads are supplied by Google. " +
        (usStatus === 3 ? "You have opted out of the sale or sharing of your personal information." : "") +
        '</p><div class="ds-row"><button type="button" class="ds-btn" data-us="1">Do Not Sell or Share My Personal Information</button></div>';
    } else {
      html +=
        '<p>Ads are supplied by Google. You can turn off personalised ads for your Google account at <a href="https://myadcenter.google.com/" rel="noopener">My Ad Center</a>, and block cookies in your browser; the site works the same.</p>';
    }
    html +=
      '<p class="ds-links"><a href="' + cfg.policy + '">Privacy policy</a> · <a href="' + cfg.cookies +
      '">Cookies</a></p><div class="ds-row"><button type="button" class="ds-btn" data-close="1">Close</button></div></div>';
    var wrap = el("div", { id: "ds-privacy", class: "ds-privacy" }, html);
    wrap.addEventListener("click", function (e) {
      var t = e.target;
      if (t === wrap || (t.getAttribute && t.getAttribute("data-close"))) return wrap.remove();
      var a = t.getAttribute && t.getAttribute("data-a");
      if (a) {
        var was = (read() || {}).analytics;
        write(a);
        if (a === "granted") grantAnalytics();
        else if (was === "granted") {
          gtag("consent", "update", { analytics_storage: "denied" });
          clearAnalyticsCookies();
          location.reload();
          return;
        }
        return openPanel();
      }
      if (t.getAttribute && t.getAttribute("data-g")) {
        wrap.remove();
        w.googlefc.showRevocationMessage();
      }
      if (t.getAttribute && t.getAttribute("data-us")) {
        wrap.remove();
        openUsOptOut();
      }
    });
    wrap.addEventListener("keydown", function (e) {
      if (e.key === "Escape") wrap.remove();
    });
    d.body.appendChild(wrap);
    var first = wrap.querySelector("button");
    if (first) first.focus();
  }
  function openUsOptOut() {
    var api = w.googlefc && w.googlefc.usstatesoptout;
    if (api && api.openConfirmationDialog) {
      api.openConfirmationDialog(function (optedOut) {
        if (optedOut) usStatus = 3;
      });
    } else {
      location.href = cfg.policy + "#your-choices";
    }
  }
  w.dynshiftPrivacy = { open: openPanel, usOptOut: openUsOptOut, gpc: gpc };

  function wireFooter() {
    var s = d.querySelectorAll("[data-privacy-settings]");
    for (var i = 0; i < s.length; i++)
      s[i].addEventListener("click", function (e) {
        e.preventDefault();
        openPanel();
      });
    var dns = d.querySelectorAll("[data-dns-link]");
    for (var j = 0; j < dns.length; j++)
      dns[j].addEventListener("click", function (e) {
        e.preventDefault();
        openUsOptOut();
      });
  }

  // Google's consent platform tells us which regime applies. If it never arrives (ad blocker, slow
  // network), treat the visitor as outside it: our own banner then asks about analytics.
  var decided = false;
  function decide() {
    if (decided) return;
    decided = true;
    decideAnalytics();
  }
  w.googlefc.callbackQueue.push({
    CONSENT_API_READY: function () {
      if (typeof w.__tcfapi !== "function") {
        if (gdprApplies === null) gdprApplies = false;
        return decide();
      }
      w.__tcfapi("addEventListener", 2, function (tc, ok) {
        if (!ok || !tc || typeof tc.gdprApplies !== "boolean") return;
        if (tc.gdprApplies && gdprApplies === false) {
          // Google's platform arrived after the fallback: its message asks, so ours steps aside.
          gdprApplies = true;
          var box = d.getElementById("ds-consent");
          if (box) box.remove();
          decided = false;
        }
        if (gdprApplies === null) gdprApplies = tc.gdprApplies;
        decide();
      });
    },
    INITIAL_US_STATES_OPT_OUT_DATA_READY: function () {
      try {
        usStatus = w.googlefc.usstatesoptout.getInitialUsStatesOptOutStatus();
      } catch (e) {
        usStatus = 0;
      }
      if (usStatus === 2 || usStatus === 3) d.documentElement.setAttribute("data-us-privacy", "");
    },
  });

  function ready() {
    wireFooter();
    mountAds();
    setTimeout(function () {
      if (gdprApplies === null) gdprApplies = false;
      decide();
    }, 4000);
  }
  if (d.readyState === "loading") d.addEventListener("DOMContentLoaded", ready);
  else ready();
})();
