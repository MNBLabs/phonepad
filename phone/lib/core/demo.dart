/// Screenshot mode.
///
/// `flutter build apk --dart-define=PHONEPAD_DEMO=true` substitutes the few
/// values that would identify the phone the screenshots were taken on. Nothing
/// else changes, so what is pictured is what ships.
library;

const kDemo = bool.fromEnvironment('PHONEPAD_DEMO');

String demoOr(String real, String demo) => kDemo ? demo : real;

const kDemoDeviceName = 'Samsung';
const kDemoAddress = '1.1.1.1';
const kDemoDeviceId = '0123456789abcdef0123456789abcdef';
