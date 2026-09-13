/** Strings that appear in more than one place, so they cannot drift apart. */

export const SITE_NAME = "PhonePad";
export const TAGLINE = "No controller? Use your phone.";
export const DESCRIPTION =
  "Turn your Android phone into a wireless Xbox-compatible controller for " +
  "Windows over Wi-Fi. No Bluetooth, no cable, nothing to buy. Works with " +
  "Xbox Cloud Gaming, Steam and Game Pass.";

export const REPO = "https://github.com/MNBLabs/phonepad";
export const RELEASES = `${REPO}/releases/latest`;
/** Stable asset names, staged by .github/workflows/release.yml. */
export const DOWNLOAD_APK = `${REPO}/releases/latest/download/PhonePad-arm64.apk`;
export const DOWNLOAD_COMPANION = `${REPO}/releases/latest/download/PhonePad-Companion.exe`;
export const DOWNLOAD_SUMS = `${REPO}/releases/latest/download/SHA256SUMS.txt`;
export const ISSUES = `${REPO}/issues`;
export const DISCUSSIONS = `${REPO}/discussions`;
export const VIGEM = "https://github.com/nefarius/ViGEmBus/releases/tag/v1.22.0";

/**
 * Measured, not claimed. Every number here has a line in docs/benchmarks.md,
 * and nothing goes on this site that the docs do not support.
 */
export const MEASURED = {
  latency: "4.9–8.0 ms",
  rate: "249/sec",
  loss: "0.0%",
  packets: "42,805",
  jitter: "0.1–0.6 ms",
};

/**
 * Filled in from repository variables at build time. Absent means the feature
 * is simply not on — no placeholder ids, no half-configured tags.
 */
export const ADSENSE_CLIENT = import.meta.env.PUBLIC_ADSENSE_CLIENT ?? "";
/** One responsive ad unit, reused on every page that carries a slot. */
export const ADSENSE_SLOT = import.meta.env.PUBLIC_ADSENSE_SLOT ?? "";
export const GA_ID = import.meta.env.PUBLIC_GA_ID ?? "";
export const SPONSORS = import.meta.env.PUBLIC_SPONSORS_URL ?? "";
export const BMC = import.meta.env.PUBLIC_BMC_URL ?? "";
