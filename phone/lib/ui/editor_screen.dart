/// Layout editor.
///
/// Built on the same layout model and the same painter as the play screen, so
/// what you arrange is literally what you play with — it is a mode over the
/// model, not a second implementation of the controller.
///
/// Edits apply to the orientation the device is currently held in. That is
/// deliberate: previewing a portrait arrangement inside a landscape canvas
/// would show a layout that does not exist, and the whole point of separate
/// per-orientation arrangements is that each is tuned for its own shape.
library;

import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../app_controller.dart';
import '../core/protocol/packets.dart';
import '../model/layout.dart';
import 'controller_painter.dart';
import 'theme.dart';
import 'touch_router.dart';

class EditorScreen extends StatefulWidget {
  const EditorScreen({
    super.key,
    required this.controller,
    required this.layout,
  });

  final AppController controller;
  final ControllerLayout layout;

  @override
  State<EditorScreen> createState() => _EditorScreenState();
}

class _EditorScreenState extends State<EditorScreen> {
  late ControllerLayout _draft;
  late final TouchRouter _preview;

  String? _selectedId;
  bool _snapToGrid = true;
  bool _dirty = false;
  Size _viewport = Size.zero;
  bool _isLandscape = true;

  /// Snapshots for undo. JSON keeps this simple and immune to aliasing bugs.
  final List<String> _undoStack = [];

  @override
  void initState() {
    super.initState();
    // Work on a copy: leaving without saving must change nothing.
    _draft = ControllerLayout.fromJson(
      jsonDecode(jsonEncode(widget.layout.toJson())) as Map<String, dynamic>,
    );
    _preview = TouchRouter(state: ControllerState());
    SystemChrome.setEnabledSystemUIMode(SystemUiMode.immersiveSticky);
  }

  @override
  void dispose() {
    _preview.dispose();
    SystemChrome.setEnabledSystemUIMode(SystemUiMode.edgeToEdge);
    super.dispose();
  }

  List<ControlSpec> get _specs =>
      _draft.forOrientation(isLandscape: _isLandscape);

  ControlSpec? get _selected {
    final id = _selectedId;
    if (id == null) return null;
    for (final s in _specs) {
      if (s.id == id) return s;
    }
    return null;
  }

  void _pushUndo() {
    _undoStack.add(jsonEncode(_draft.toJson()));
    if (_undoStack.length > 40) _undoStack.removeAt(0);
  }

  void _undo() {
    if (_undoStack.isEmpty) return;
    final snapshot = _undoStack.removeLast();
    setState(() {
      _draft = ControllerLayout.fromJson(
        jsonDecode(snapshot) as Map<String, dynamic>,
      );
      _dirty = true;
      _refreshGeometry();
    });
  }

  void _refreshGeometry() {
    if (_viewport == Size.zero) return;
    // Hidden controls still need to be reachable in the editor, otherwise
    // unhiding one would be impossible.
    final visibleForEditing = _specs.map((s) {
      final copy = s.copy();
      copy.visible = true;
      copy.opacity = s.visible ? s.opacity : 0.18;
      return copy;
    }).toList();

    _preview.geometry = resolveGeometry(
      visibleForEditing,
      _viewport.width,
      _viewport.height,
    );
    _preview.notify();
  }

  void _mutate(void Function() change, {bool snapshot = true}) {
    if (snapshot) _pushUndo();
    setState(() {
      change();
      _dirty = true;
      _refreshGeometry();
    });
  }

  Future<void> _save() async {
    await widget.controller.saveLayout(_draft);
    if (!mounted) return;
    setState(() => _dirty = false);
    ScaffoldMessenger.of(context).showSnackBar(
      const SnackBar(
        content: Text('Layout saved'),
        duration: Duration(seconds: 1),
      ),
    );
  }

  Future<bool> _confirmDiscard() async {
    if (!_dirty) return true;
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        backgroundColor: kNightHi,
        title: const Text('Discard changes?'),
        content: const Text(
          'This layout has unsaved changes.',
          style: TextStyle(color: kNightInk2),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Keep editing'),
          ),
          TextButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Discard', style: TextStyle(color: kNightBad)),
          ),
          FilledButton(
            // Resolve the navigator before awaiting, so no BuildContext is
            // used across the gap.
            onPressed: () async {
              final navigator = Navigator.of(context);
              await _save();
              navigator.pop(true);
            },
            child: const Text('Save'),
          ),
        ],
      ),
    );
    return ok ?? false;
  }

  // --- canvas interaction ----------------------------------------------------

  void _onTapDown(Offset pos) {
    for (var i = _preview.geometry.length - 1; i >= 0; i--) {
      final g = _preview.geometry[i];
      if (g.hitTest(pos.dx, pos.dy, slop: 1.0)) {
        setState(() => _selectedId = g.spec.id);
        return;
      }
    }
    setState(() => _selectedId = null);
  }

  void _onDrag(Offset delta) {
    final spec = _selected;
    if (spec == null || _viewport == Size.zero) return;
    // One undo entry per drag, not per frame.
    _mutate(snapshot: false, () {
      spec.x = (spec.x + delta.dx / _viewport.width).clamp(0.02, 0.98);
      spec.y = (spec.y + delta.dy / _viewport.height).clamp(0.02, 0.98);
    });
  }

  void _snapSelected() {
    final spec = _selected;
    if (spec == null || !_snapToGrid) return;
    _mutate(snapshot: false, () {
      spec.x = (spec.x * 40).round() / 40;
      spec.y = (spec.y * 24).round() / 24;
    });
  }

  // --- control management ----------------------------------------------------

  void _addControl(ControlType type) {
    final id = '${type.name}-${DateTime.now().microsecondsSinceEpoch}';
    _mutate(() {
      _specs.add(
        ControlSpec(
          id: id,
          type: type,
          mapping: switch (type) {
            ControlType.button => const ControlMapping(buttons: Btn.a),
            ControlType.trigger => const ControlMapping(
              trigger: StickSide.left,
            ),
            ControlType.stick => const ControlMapping(stick: StickSide.left),
            ControlType.dpad => const ControlMapping(),
          },
          x: 0.5,
          y: 0.5,
          size: switch (type) {
            ControlType.stick => 0.32,
            ControlType.dpad => 0.26,
            ControlType.trigger => 0.125,
            ControlType.button => 0.115,
          },
          aspect: type == ControlType.trigger ? 1.5 : 1.0,
          shape: type == ControlType.trigger
              ? ControlShape.roundedRect
              : ControlShape.circle,
          label: switch (type) {
            ControlType.button => 'A',
            ControlType.trigger => 'LT',
            ControlType.stick => 'L',
            ControlType.dpad => '',
          },
        ),
      );
      _selectedId = id;
    });
  }

  void _duplicateSelected() {
    final spec = _selected;
    if (spec == null) return;
    final copy = spec.copy()
      ..id = '${spec.type.name}-${DateTime.now().microsecondsSinceEpoch}'
      ..x = (spec.x + 0.06).clamp(0.02, 0.98)
      ..y = (spec.y + 0.06).clamp(0.02, 0.98);
    _mutate(() {
      _specs.add(copy);
      _selectedId = copy.id;
    });
  }

  void _deleteSelected() {
    final spec = _selected;
    if (spec == null) return;
    _mutate(() {
      _specs.removeWhere((s) => s.id == spec.id);
      _selectedId = null;
    });
  }

  @override
  Widget build(BuildContext context) {
    return Theme(
      data: buildNightTheme(),
      child: PopScope(
        canPop: false,
        onPopInvokedWithResult: (didPop, _) async {
          if (didPop) return;
          // Resolve the navigator up front so no BuildContext crosses the await.
          final navigator = Navigator.of(context);
          if (await _confirmDiscard()) navigator.pop();
        },
        child: Scaffold(
          backgroundColor: kNight,
          body: SafeArea(
            child: LayoutBuilder(
              builder: (context, constraints) {
                final size = Size(constraints.maxWidth, constraints.maxHeight);
                final landscape = size.width >= size.height;
                if (size != _viewport || landscape != _isLandscape) {
                  _viewport = size;
                  _isLandscape = landscape;
                  WidgetsBinding.instance.addPostFrameCallback(
                    (_) => setState(_refreshGeometry),
                  );
                }

                return Stack(
                  children: [
                    Positioned.fill(
                      child: GestureDetector(
                        behavior: HitTestBehavior.opaque,
                        onTapDown: (d) => _onTapDown(d.localPosition),
                        onPanStart: (d) {
                          _onTapDown(d.localPosition);
                          if (_selected != null) _pushUndo();
                        },
                        onPanUpdate: (d) => _onDrag(d.delta),
                        onPanEnd: (_) => _snapSelected(),
                        child: RepaintBoundary(
                          child: CustomPaint(
                            size: Size.infinite,
                            painter: ControllerPainter(
                              router: _preview,
                              theme: const ControllerTheme(),
                              editorSelection: _selectedId,
                              showGrid: _snapToGrid,
                            ),
                          ),
                        ),
                      ),
                    ),
                    _buildToolbar(),
                    if (_selected != null) _buildInspector(_selected!),
                  ],
                );
              },
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildToolbar() {
    return Positioned(
      top: 0,
      left: 0,
      right: 0,
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
        color: kNight.withValues(alpha: 0.85),
        child: Row(
          children: [
            IconButton(
              icon: const Icon(Icons.arrow_back),
              onPressed: () async {
                final proceed = await _confirmDiscard();
                if (!mounted || !proceed) return;
                Navigator.of(context).pop();
              },
            ),
            Expanded(
              child: Text(
                '${_draft.name} · ${_isLandscape ? "landscape" : "portrait"}',
                style: const TextStyle(
                  fontSize: 14,
                  fontWeight: FontWeight.w600,
                ),
                overflow: TextOverflow.ellipsis,
              ),
            ),
            IconButton(
              tooltip: 'Snap to grid',
              icon: Icon(
                Icons.grid_4x4,
                color: _snapToGrid ? kBrand : kNightInk2,
              ),
              onPressed: () => setState(() => _snapToGrid = !_snapToGrid),
            ),
            IconButton(
              tooltip: 'Undo',
              icon: const Icon(Icons.undo),
              onPressed: _undoStack.isEmpty ? null : _undo,
            ),
            PopupMenuButton<ControlType>(
              tooltip: 'Add control',
              color: kNightHi,
              icon: const Icon(Icons.add_circle_outline, color: kBrand),
              onSelected: _addControl,
              itemBuilder: (context) => const [
                PopupMenuItem(value: ControlType.button, child: Text('Button')),
                PopupMenuItem(value: ControlType.stick, child: Text('Stick')),
                PopupMenuItem(value: ControlType.dpad, child: Text('D-pad')),
                PopupMenuItem(
                  value: ControlType.trigger,
                  child: Text('Trigger'),
                ),
              ],
            ),
            FilledButton(
              onPressed: _dirty ? _save : null,
              child: const Text('Save'),
            ),
            const SizedBox(width: 4),
          ],
        ),
      ),
    );
  }

  Widget _buildInspector(ControlSpec spec) {
    return Positioned(
      right: 0,
      bottom: 0,
      top: 52,
      width: 300,
      child: Container(
        color: kNightHi.withValues(alpha: 0.96),
        child: ListView(
          padding: const EdgeInsets.all(12),
          children: [
            Row(
              children: [
                Expanded(
                  child: Text(
                    spec.type.name.toUpperCase(),
                    style: const TextStyle(
                      color: kBrand,
                      fontWeight: FontWeight.w700,
                      letterSpacing: 1,
                      fontSize: 12,
                    ),
                  ),
                ),
                IconButton(
                  tooltip: spec.visible ? 'Hide' : 'Show',
                  icon: Icon(
                    spec.visible ? Icons.visibility : Icons.visibility_off,
                    size: 20,
                  ),
                  onPressed: () => _mutate(() => spec.visible = !spec.visible),
                ),
                IconButton(
                  tooltip: 'Duplicate',
                  icon: const Icon(Icons.copy_all_outlined, size: 20),
                  onPressed: _duplicateSelected,
                ),
                IconButton(
                  tooltip: 'Delete',
                  icon: const Icon(
                    Icons.delete_outline,
                    size: 20,
                    color: kNightBad,
                  ),
                  onPressed: _deleteSelected,
                ),
              ],
            ),

            if (spec.type == ControlType.button) _buttonMapping(spec),
            if (spec.type == ControlType.stick ||
                spec.type == ControlType.trigger)
              _sideMapping(spec),

            _labelField(spec),
            _slider('Size', spec.size, 0.04, 0.6, (v) => spec.size = v),
            if (spec.shape != ControlShape.circle)
              _slider(
                'Width ratio',
                spec.aspect,
                0.4,
                3.5,
                (v) => spec.aspect = v,
              ),
            _slider(
              'Opacity',
              spec.opacity,
              0.05,
              1.0,
              (v) => spec.opacity = v,
            ),
            _slider(
              'Rotation',
              spec.rotation,
              -0.8,
              0.8,
              (v) => spec.rotation = v,
            ),

            _shapePicker(spec),

            if (spec.type == ControlType.stick) ...[
              const Divider(height: 24),
              _slider(
                'Deadzone',
                spec.deadzone,
                0.0,
                0.5,
                (v) => spec.deadzone = v,
              ),
              _slider(
                'Sensitivity',
                spec.sensitivity,
                0.3,
                2.5,
                (v) => spec.sensitivity = v,
              ),
              _slider(
                'Response curve',
                spec.responseCurve,
                0.5,
                2.5,
                (v) => spec.responseCurve = v,
              ),
              SwitchListTile(
                contentPadding: EdgeInsets.zero,
                dense: true,
                title: const Text(
                  'Circular range',
                  style: TextStyle(fontSize: 13),
                ),
                value: spec.circularRange,
                onChanged: (v) => _mutate(() => spec.circularRange = v),
              ),
              SwitchListTile(
                contentPadding: EdgeInsets.zero,
                dense: true,
                title: const Text('Floating', style: TextStyle(fontSize: 13)),
                subtitle: const Text(
                  'Recentres where your thumb lands',
                  style: TextStyle(color: kNightInk2, fontSize: 11),
                ),
                value: spec.floating,
                onChanged: (v) => _mutate(() => spec.floating = v),
              ),
            ],

            if (spec.type == ControlType.dpad) ...[
              const Divider(height: 24),
              _slider(
                'Deadzone',
                spec.deadzone,
                0.05,
                0.6,
                (v) => spec.deadzone = v,
              ),
            ],

            if (spec.type == ControlType.trigger) ...[
              const Divider(height: 24),
              SwitchListTile(
                contentPadding: EdgeInsets.zero,
                dense: true,
                title: const Text(
                  'Analogue slide',
                  style: TextStyle(fontSize: 13),
                ),
                subtitle: const Text(
                  'Slide down for partial pull instead of full press',
                  style: TextStyle(color: kNightInk2, fontSize: 11),
                ),
                value: spec.analogSlide,
                onChanged: (v) => _mutate(() => spec.analogSlide = v),
              ),
              if (spec.analogSlide)
                _slider(
                  'Trigger curve',
                  spec.responseCurve,
                  0.5,
                  2.5,
                  (v) => spec.responseCurve = v,
                ),
            ],

            const SizedBox(height: 12),
            const Text(
              'Drag on the canvas to move the selected control.',
              style: TextStyle(color: kNightInk2, fontSize: 11),
            ),
          ],
        ),
      ),
    );
  }

  Widget _labelField(ControlSpec spec) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 8),
      child: TextFormField(
        initialValue: spec.label,
        // The key ties the field to the control, so selecting a different one
        // refreshes the text instead of keeping the previous control's label.
        key: ValueKey('label-${spec.id}'),
        decoration: const InputDecoration(
          labelText: 'Label',
          isDense: true,
          contentPadding: EdgeInsets.symmetric(horizontal: 12, vertical: 10),
        ),
        style: const TextStyle(fontSize: 14),
        onChanged: (v) => _mutate(snapshot: false, () => spec.label = v),
      ),
    );
  }

  Widget _slider(
    String label,
    double value,
    double min,
    double max,
    void Function(double) apply,
  ) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          '$label: ${value.toStringAsFixed(2)}',
          style: const TextStyle(color: kNightInk2, fontSize: 12),
        ),
        SliderTheme(
          data: SliderTheme.of(context).copyWith(trackHeight: 2),
          child: Slider(
            value: value.clamp(min, max),
            min: min,
            max: max,
            // Live feedback while dragging, one undo entry when it ends.
            onChangeStart: (_) => _pushUndo(),
            onChanged: (v) => _mutate(snapshot: false, () => apply(v)),
          ),
        ),
      ],
    );
  }

  Widget _shapePicker(ControlSpec spec) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: SegmentedButton<ControlShape>(
        showSelectedIcon: false,
        style: ButtonStyle(
          textStyle: WidgetStatePropertyAll(
            Theme.of(context).textTheme.labelSmall,
          ),
        ),
        segments: const [
          ButtonSegment(value: ControlShape.circle, label: Text('Circle')),
          ButtonSegment(value: ControlShape.roundedRect, label: Text('Rect')),
          ButtonSegment(value: ControlShape.pill, label: Text('Pill')),
        ],
        selected: {spec.shape},
        onSelectionChanged: (s) => _mutate(() => spec.shape = s.first),
      ),
    );
  }

  Widget _buttonMapping(ControlSpec spec) {
    const options = {
      'A': Btn.a,
      'B': Btn.b,
      'X': Btn.x,
      'Y': Btn.y,
      'LB': Btn.lb,
      'RB': Btn.rb,
      'L3': Btn.ls,
      'R3': Btn.rs,
      'Menu': Btn.start,
      'View': Btn.back,
      'Guide': Btn.guide,
      'D-up': Btn.dpadUp,
      'D-down': Btn.dpadDown,
      'D-left': Btn.dpadLeft,
      'D-right': Btn.dpadRight,
    };
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 8),
      child: DropdownButtonFormField<int>(
        initialValue: options.containsValue(spec.mapping.buttons)
            ? spec.mapping.buttons
            : null,
        isDense: true,
        decoration: const InputDecoration(
          labelText: 'Sends',
          isDense: true,
          contentPadding: EdgeInsets.symmetric(horizontal: 12, vertical: 10),
        ),
        dropdownColor: kNightHi,
        items: [
          for (final e in options.entries)
            DropdownMenuItem(value: e.value, child: Text(e.key)),
        ],
        onChanged: (v) {
          if (v == null) return;
          _mutate(() {
            spec.mapping = ControlMapping(buttons: v);
            final name = options.entries.firstWhere((e) => e.value == v).key;
            if (spec.label.isEmpty || options.containsKey(spec.label)) {
              spec.label = name;
            }
          });
        },
      ),
    );
  }

  Widget _sideMapping(ControlSpec spec) {
    final isStick = spec.type == ControlType.stick;
    final current = isStick ? spec.mapping.stick : spec.mapping.trigger;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: SegmentedButton<StickSide>(
        showSelectedIcon: false,
        segments: [
          ButtonSegment(
            value: StickSide.left,
            label: Text(isStick ? 'Left stick' : 'LT'),
          ),
          ButtonSegment(
            value: StickSide.right,
            label: Text(isStick ? 'Right stick' : 'RT'),
          ),
        ],
        selected: {current ?? StickSide.left},
        onSelectionChanged: (s) => _mutate(() {
          spec.mapping = isStick
              ? ControlMapping(stick: s.first)
              : ControlMapping(trigger: s.first);
          spec.label = isStick
              ? (s.first == StickSide.left ? 'L' : 'R')
              : (s.first == StickSide.left ? 'LT' : 'RT');
        }),
      ),
    );
  }
}
