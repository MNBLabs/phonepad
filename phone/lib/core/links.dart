/// Outward-facing URLs, in one place.
///
/// These are the only strings in the app that point at anything off the
/// device, so they live together rather than being scattered through screens.
/// Anything that depends on an account that may not exist yet is routed
/// through the site, so a link can be repointed without shipping a new build.
library;

import 'package:flutter/services.dart';

const kSiteUrl = 'https://phonepad.dynshift.com';
const kProjectUrl = 'https://github.com/MNBLabs/phonepad';
const kSupportUrl = '$kSiteUrl/support/';
const kSetupUrl = '$kSiteUrl/setup/';
const kTroubleshootingUrl = '$kSiteUrl/troubleshooting/';

/// The ViGEmBus release the companion needs. Pinned rather than "latest": the
/// setup instructions name this version, so the link has to agree with them.
const kVigemUrl =
    'https://github.com/nefarius/ViGEmBus/releases/tag/v1.22.0';

/// Open a URL in the browser.
///
/// Goes through the existing platform channel rather than adding a package for
/// one call. Failure is silent by design — a link that will not open is not
/// worth interrupting a game session over, and every destination is also
/// reachable from the PC.
Future<void> launchProjectUrl(String url) async {
  const channel = MethodChannel('dev.phonepad/platform');
  try {
    await channel.invokeMethod<void>('openUrl', {'url': url});
  } on PlatformException {
    // Nothing useful to say to the user here.
  } on MissingPluginException {
    // Running on a host without the Android side, e.g. a widget test.
  }
}
