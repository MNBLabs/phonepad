import 'package:flutter/material.dart';

import '../app_controller.dart';
import '../model/layout.dart';
import 'editor_screen.dart';
import 'theme.dart';

class LayoutsScreen extends StatefulWidget {
  const LayoutsScreen({super.key, required this.controller});

  final AppController controller;

  @override
  State<LayoutsScreen> createState() => _LayoutsScreenState();
}

class _LayoutsScreenState extends State<LayoutsScreen> {
  AppController get c => widget.controller;

  Future<String?> _askName(String title, String initial) {
    final field = TextEditingController(text: initial);
    return showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        backgroundColor: kRaised,
        title: Text(title),
        content: TextField(
          controller: field,
          autofocus: true,
          onSubmitted: (v) => Navigator.of(context).pop(v.trim()),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(field.text.trim()),
            child: const Text('Save'),
          ),
        ],
      ),
    );
  }

  Future<void> _confirmDelete(ControllerLayout layout) async {
    final isPreset = layout.id.startsWith('builtin-');
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        backgroundColor: kRaised,
        title: Text(
          isPreset ? 'Reset "${layout.name}"?' : 'Delete "${layout.name}"?',
        ),
        content: Text(
          isPreset
              ? 'Your changes to this built-in layout will be discarded and the '
                    'original restored.'
              : 'This layout will be permanently removed.',
          style: const TextStyle(color: kInk2),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            style: FilledButton.styleFrom(backgroundColor: kBad),
            onPressed: () => Navigator.of(context).pop(true),
            child: Text(isPreset ? 'Reset' : 'Delete'),
          ),
        ],
      ),
    );
    if (ok != true) return;
    if (isPreset) {
      await c.resetLayout(layout);
    } else {
      await c.deleteLayout(layout);
    }
    if (mounted) setState(() {});
  }

  @override
  Widget build(BuildContext context) {
    final active = c.activeLayout;

    return Scaffold(
      appBar: AppBar(title: const Text('Layouts')),
      body: ListView(
        padding: const EdgeInsets.fromLTRB(16, 8, 16, 32),
        children: [
          const Panel(
            child: Text(
              'Tap a layout to make it active. Each one holds a separate '
              'arrangement for landscape and portrait, and switches '
              'automatically when you rotate.',
              style: TextStyle(color: kInk2, fontSize: 13),
            ),
          ),
          const SizedBox(height: 16),
          for (final layout in c.layouts) ...[
            _LayoutTile(
              layout: layout,
              active: layout.id == active.id,
              onSelect: () async {
                await c.setActiveLayout(layout.id);
                if (mounted) setState(() {});
              },
              onEdit: () async {
                await Navigator.of(context).push(
                  MaterialPageRoute(
                    builder: (_) => EditorScreen(controller: c, layout: layout),
                  ),
                );
                if (mounted) setState(() {});
              },
              onDuplicate: () async {
                final name = await _askName(
                  'Duplicate layout',
                  '${layout.name} copy',
                );
                if (name == null || name.isEmpty) return;
                await c.duplicateLayout(layout, name);
                if (mounted) setState(() {});
              },
              onRename: () async {
                final name = await _askName('Rename layout', layout.name);
                if (name == null || name.isEmpty) return;
                await c.renameLayout(layout, name);
                if (mounted) setState(() {});
              },
              onDelete: () => _confirmDelete(layout),
            ),
            const SizedBox(height: 10),
          ],
        ],
      ),
    );
  }
}

class _LayoutTile extends StatelessWidget {
  const _LayoutTile({
    required this.layout,
    required this.active,
    required this.onSelect,
    required this.onEdit,
    required this.onDuplicate,
    required this.onRename,
    required this.onDelete,
  });

  final ControllerLayout layout;
  final bool active;
  final VoidCallback onSelect;
  final VoidCallback onEdit;
  final VoidCallback onDuplicate;
  final VoidCallback onRename;
  final VoidCallback onDelete;

  @override
  Widget build(BuildContext context) {
    final isPreset = layout.id.startsWith('builtin-');
    final edited = isPreset && !layout.builtIn;

    return Container(
      decoration: BoxDecoration(
        color: kRaised,
        borderRadius: BorderRadius.circular(16),
        border: Border.all(color: active ? kAccent : Colors.transparent),
      ),
      child: Column(
        children: [
          ListTile(
            onTap: onSelect,
            leading: Icon(
              active
                  ? Icons.radio_button_checked
                  : Icons.radio_button_unchecked,
              color: active ? kAccent : kInk2,
            ),
            title: Text(layout.name),
            subtitle: Text(
              [
                '${layout.landscape.length} controls',
                if (isPreset) edited ? 'built-in, edited' : 'built-in',
              ].join(' · '),
              style: const TextStyle(color: kInk2, fontSize: 12),
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(8, 0, 8, 8),
            child: Row(
              children: [
                TextButton.icon(
                  onPressed: onEdit,
                  icon: const Icon(Icons.edit_outlined, size: 18),
                  label: const Text('Edit'),
                ),
                TextButton.icon(
                  onPressed: onDuplicate,
                  icon: const Icon(Icons.copy_all_outlined, size: 18),
                  label: const Text('Copy'),
                ),
                const Spacer(),
                PopupMenuButton<String>(
                  color: kSunken,
                  icon: const Icon(Icons.more_vert, color: kInk2),
                  onSelected: (v) {
                    if (v == 'rename') onRename();
                    if (v == 'delete') onDelete();
                  },
                  itemBuilder: (context) => [
                    const PopupMenuItem(value: 'rename', child: Text('Rename')),
                    PopupMenuItem(
                      value: 'delete',
                      child: Text(
                        isPreset ? 'Reset to default' : 'Delete',
                        style: const TextStyle(color: kBad),
                      ),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
