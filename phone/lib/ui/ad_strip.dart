/// The strip at the bottom of the setup screens.
///
/// It is a slot of its own (`Scaffold.bottomNavigationBar`), so the screen's
/// content ends above it and scrolls; nothing is ever drawn underneath an ad
/// and an ad never covers a control. Its height is settled before any ad
/// loads, so nothing jumps when one arrives, and whenever there is no ad (a
/// build without one, consent not given, no fill, no network) the same space
/// carries PhonePad's own placement instead of going blank.
library;

import 'package:flutter/material.dart';
import 'package:google_mobile_ads/google_mobile_ads.dart';

import '../core/ads.dart';
import '../core/links.dart';
import 'theme.dart';

/// Height when there is no ad size to match: a standard banner plus margins.
const _houseHeight = 60.0;

/// Taller adaptive sizes exist; a strip that takes more than this stops being
/// a strip. Above it the plain 320×50 banner is used instead.
const _maxAdHeight = 90;

/// The most of the usable height (below the system bars) the strip may take.
const _maxShare = 0.12;

/// Space above and below the banner inside the strip.
const _margin = 8.0;

class AdStrip extends StatefulWidget {
  const AdStrip({super.key});

  @override
  State<AdStrip> createState() => _AdStripState();
}

class _AdStripState extends State<AdStrip> {
  BannerAd? _ad;
  AdSize? _size;
  bool _loaded = false;

  /// What the current ad was requested for; a change means a new request.
  int? _forWidth;
  Orientation? _forOrientation;

  @override
  void initState() {
    super.initState();
    Ads.instance.addListener(_onAdsChanged);
  }

  @override
  void dispose() {
    Ads.instance.removeListener(_onAdsChanged);
    _ad?.dispose();
    super.dispose();
  }

  void _onAdsChanged() {
    if (mounted) setState(() {});
  }

  Future<void> _load(int width, Orientation orientation, int room) async {
    _forWidth = width;
    _forOrientation = orientation;
    _ad?.dispose();
    _ad = null;
    _loaded = false;

    // The standard anchored adaptive size, not the newer "large" one, which
    // may take up to 15% of the screen.
    // ignore: deprecated_member_use
    final adaptive = await AdSize.getAnchoredAdaptiveBannerAdSize(orientation, width);
    final limit = room < _maxAdHeight ? room : _maxAdHeight;
    final size = (adaptive != null && adaptive.height <= limit)
        ? adaptive
        : AdSize.banner;
    if (!mounted || _forWidth != width || _forOrientation != orientation) return;
    setState(() => _size = size);

    final ad = BannerAd(
      adUnitId: kAdmobBannerId,
      size: size,
      request: const AdRequest(),
      listener: BannerAdListener(
        onAdLoaded: (_) {
          if (mounted) setState(() => _loaded = true);
        },
        onAdFailedToLoad: (ad, error) {
          // No fill, no network, a policy hold: the house placement stays.
          debugPrint('PhonePad: banner not loaded: ${error.message}');
          ad.dispose();
          if (mounted) {
            setState(() {
              _ad = null;
              _loaded = false;
            });
          }
        },
      ),
    );
    _ad = ad;
    await ad.load();
  }

  @override
  Widget build(BuildContext context) {
    final media = MediaQuery.of(context);
    final width = media.size.width.truncate();
    final orientation = media.orientation;

    // Clear the system's bottom edge as well as any inset. Measured on a
    // Galaxy S24 Ultra with gesture navigation: the navigation-bar inset is
    // zero and the gesture hint is drawn over the app, so SafeArea alone
    // would put the ad on the home-swipe zone, where accidental taps happen.
    final bottom = [
      media.viewPadding.bottom,
      media.systemGestureInsets.bottom,
    ].reduce((a, b) => a > b ? a : b);

    // The strip may take at most this share of the screen. A phone in
    // landscape cannot fit even the smallest banner within it (measured on
    // the S24 Ultra: about a quarter of the height), and there the strip is
    // simply not shown, and no ad is requested.
    final room = (media.size.height - bottom) * _maxShare - _margin;
    if (room < AdSize.banner.height) return const SizedBox.shrink();

    if (Ads.instance.state == AdsState.ready &&
        (width != _forWidth || orientation != _forOrientation)) {
      // Outside build: a request cannot be issued from inside one.
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _load(width, orientation, room.floor());
      });
    }

    final ad = _ad;
    final showingAd = _loaded && ad != null && Ads.instance.state == AdsState.ready;
    final height = (_size?.height.toDouble() ?? 0) > 0
        ? _size!.height.toDouble() + _margin
        : _houseHeight;

    return DecoratedBox(
      decoration: const BoxDecoration(
        color: kRaised,
        border: Border(top: BorderSide(color: kLine)),
      ),
      child: Padding(
        padding: EdgeInsets.only(
          bottom: bottom,
          left: media.viewPadding.left,
          right: media.viewPadding.right,
        ),
        child: SizedBox(
          height: height,
          child: showingAd
              ? Center(
                  child: SizedBox(
                    width: ad.size.width.toDouble(),
                    height: ad.size.height.toDouble(),
                    child: AdWidget(ad: ad),
                  ),
                )
              : const _HousePlacement(),
        ),
      ),
    );
  }
}

/// PhonePad's own placement: what the strip says when there is no ad.
class _HousePlacement extends StatelessWidget {
  const _HousePlacement();

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 20),
      child: Row(
        children: [
          const Expanded(
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'PhonePad is free and open source',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    color: kInk,
                    fontSize: 14,
                    fontWeight: FontWeight.w600,
                  ),
                ),
                SizedBox(height: 2),
                Text(
                  'This strip keeps it that way.',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(color: kInk2, fontSize: 12.5),
                ),
              ],
            ),
          ),
          const SizedBox(width: 12),
          TextButton(
            onPressed: () => launchProjectUrl(kSupportUrl),
            style: TextButton.styleFrom(foregroundColor: kAccent),
            child: const Text('Support'),
          ),
        ],
      ),
    );
  }
}
