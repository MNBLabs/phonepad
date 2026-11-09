/// Advertising: one banner, in a strip of its own, on the setup screens only.
///
/// Never on the controller, the editor or the first-run pages. Those are where
/// hands are busy and taps are meant for something else, and an ad there would
/// be both a worse product and an accidental-click policy violation.
///
/// Configuration comes from the build, not the source:
///
/// ```
/// flutter build apk --dart-define=PHONEPAD_ADMOB_BANNER_ID=ca-app-pub-…/…
/// ```
///
/// plus the AdMob *app* id in the `PHONEPAD_ADMOB_APP_ID` environment variable
/// for Gradle (see android/app/build.gradle.kts). A build without them, which
/// is any build from source, never initialises the ad SDK and never makes an ad
/// request; the strip shows PhonePad's own placement instead.
///
/// Consent follows Google's own messaging platform (UMP), which is what AdMob
/// requires for the EEA, the UK and Switzerland: the form is shown where the law
/// asks for it, ads are only requested once `canRequestAds` says so, and the
/// choice can be changed later from Settings.
library;

import 'package:flutter/foundation.dart';
import 'package:google_mobile_ads/google_mobile_ads.dart';

/// The banner ad unit. Empty in builds without ads.
const kAdmobBannerId = String.fromEnvironment('PHONEPAD_ADMOB_BANNER_ID');

/// Whether this build carries an ad unit at all.
const kAdsConfigured = kAdmobBannerId != '';

enum AdsState {
  /// No ad unit in this build. The strip shows the house placement.
  off,

  /// Waiting for the consent check.
  starting,

  /// Ads may be requested.
  ready,

  /// Consent was not given, or the check failed. No ad requests are made.
  unavailable,
}

class Ads extends ChangeNotifier {
  Ads._();

  static final Ads instance = Ads._();

  AdsState _state = kAdsConfigured ? AdsState.starting : AdsState.off;
  bool _privacyOptionsRequired = false;
  bool _started = false;

  AdsState get state => _state;

  /// Whether Settings must offer a way to revisit the consent choice.
  bool get privacyOptionsRequired => _privacyOptionsRequired;

  void _set(AdsState s) {
    if (_state == s) return;
    _state = s;
    notifyListeners();
  }

  /// Run the consent check, then start the SDK if ads may be shown. Called
  /// once, after the first frame, because the consent form needs an activity.
  /// Never throws: any failure simply leaves the house placement in the strip.
  Future<void> start() async {
    if (!kAdsConfigured || _started) return;
    _started = true;
    try {
      ConsentInformation.instance.requestConsentInfoUpdate(
        ConsentRequestParameters(),
        () async {
          await ConsentForm.loadAndShowConsentFormIfRequired((error) {
            if (error != null) debugPrint('PhonePad: consent form: ${error.message}');
          });
          await _afterConsent();
        },
        (error) async {
          debugPrint('PhonePad: consent check failed: ${error.message}');
          // A previous session's answer may still allow ads.
          await _afterConsent();
        },
      );
    } catch (e) {
      debugPrint('PhonePad: ads unavailable: $e');
      _set(AdsState.unavailable);
    }
  }

  Future<void> _afterConsent() async {
    try {
      _privacyOptionsRequired =
          await ConsentInformation.instance.getPrivacyOptionsRequirementStatus() ==
              PrivacyOptionsRequirementStatus.required;
      if (await ConsentInformation.instance.canRequestAds()) {
        await MobileAds.instance.initialize();
        _set(AdsState.ready);
      } else {
        _set(AdsState.unavailable);
      }
    } catch (e) {
      debugPrint('PhonePad: ads unavailable: $e');
      _set(AdsState.unavailable);
    }
    notifyListeners();
  }

  /// The "privacy choices" form, from Settings.
  Future<void> showPrivacyOptions() async {
    await ConsentForm.showPrivacyOptionsForm((error) {
      if (error != null) debugPrint('PhonePad: privacy options: ${error.message}');
    });
    await _afterConsent();
  }
}
