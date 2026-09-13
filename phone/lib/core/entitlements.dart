/// What this build is allowed to do.
///
/// PhonePad is free and the controller is not going behind a paywall. This
/// exists so that if paid extras ever ship — themes, motion controls, deeper
/// tuning — they arrive as one implementation of this interface rather than as
/// `isPro` checks scattered through the screens.
///
/// The open-source build uses [FreeEntitlements], which grants everything that
/// exists today. A feature only appears here once there is something to gate;
/// listing hypothetical ones would be advertising for a product that does not
/// exist.
library;

enum Feature {
  /// Motion aiming. Not implemented in any build yet; see ROADMAP.md.
  gyro,

  /// Theming the on-screen controller beyond opacity.
  themes,
}

abstract interface class Entitlements {
  bool has(Feature feature);
}

/// The open-source build.
///
/// Everything that ships is available. Nothing that already works has been
/// moved behind this: the free app is the product, and a gate that took
/// something away would be a regression, not a business model.
class FreeEntitlements implements Entitlements {
  const FreeEntitlements();

  @override
  bool has(Feature feature) => switch (feature) {
        // Absent because it is unimplemented, not because it is withheld.
        Feature.gyro => false,
        Feature.themes => false,
      };
}
