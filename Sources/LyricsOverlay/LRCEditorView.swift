import AppKit
import LyricsCore
import SwiftUI

@MainActor
struct LRCEditorView: View {
    @Environment(\.dismiss) private var dismiss
    @StateObject private var editor = LRCEditorController()
    @State private var text: String
    @State private var validation: Validation
    @State private var actionError: String?

    private let initialText: String
    private let position: () -> TimeInterval?
    private let save: (String) throws -> Void

    init(initialText: String, position: @escaping () -> TimeInterval?, save: @escaping (String) throws -> Void) {
        self.initialText = initialText
        self.position = position
        self.save = save
        _text = State(initialValue: initialText)
        _validation = State(initialValue: Self.validate(initialText))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Eigene Lyrics bearbeiten")
                .font(.headline)
            Text("Füge deinen Text ein und setze beim Abspielen den Zeitstempel für die Zeile am Cursor.")
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)

            LRCTextEditor(text: $text, editor: editor)
                .frame(minHeight: 230)
                .overlay {
                    RoundedRectangle(cornerRadius: 5)
                        .strokeBorder(.secondary.opacity(0.28))
                        .allowsHitTesting(false)
                }

            HStack {
                Button {
                    insertTimestamp()
                } label: {
                    Label("Zeitstempel setzen", systemImage: "clock")
                }
                .keyboardShortcut("t", modifiers: [.command, .shift])
                .help("Setzt den Zeitstempel und springt zur nächsten Zeile (⇧⌘T)")
                .disabled(validation == .tooLarge)

                Spacer()
                Text(validation.message)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            if let actionError {
                Text(actionError)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .fixedSize(horizontal: false, vertical: true)
            }

            HStack {
                Spacer()
                Button("Abbrechen") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("Speichern") { saveDraft() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(!validation.canSave || text == initialText)
            }
        }
        .padding(20)
        .frame(width: 500)
        .onChange(of: text) { text in
            validation = Self.validate(text)
            actionError = nil
        }
    }

    private func insertTimestamp() {
        guard let time = position(), time.isFinite else {
            actionError = "Spiele diesen Song ab, um seine aktuelle Wiedergabezeit zu übernehmen."
            return
        }
        if !editor.stamp(at: time) {
            actionError = "Setze den Cursor in eine Songzeile."
        }
    }

    private func saveDraft() {
        guard Self.validate(text).canSave else { return }
        do {
            try save(text)
            dismiss()
        } catch {
            actionError = "Die eigenen Lyrics konnten nicht gespeichert werden."
        }
    }

    private enum Validation: Equatable {
        case noLyrics
        case tooLarge
        case valid(Int)

        var canSave: Bool {
            if case .valid = self { return true }
            return false
        }

        var message: String {
            switch self {
            case .noLyrics: return "Mindestens eine Zeile mit Zeitstempel fehlt."
            case .tooLarge: return "Maximal 512 KB pro Song."
            case .valid(let count): return count == 1 ? "1 synchronisierte Zeile" : "\(count) synchronisierte Zeilen"
            }
        }
    }

    private static func validate(_ text: String) -> Validation {
        guard text.utf8.count <= TrackLyricsStore.maximumLyricsBytes else { return .tooLarge }
        let count = LRC.parse(text).filter { !$0.text.isEmpty }.count
        return count == 0 ? .noLyrics : .valid(count)
    }
}

@MainActor
private final class LRCEditorController: ObservableObject {
    weak var textView: NSTextView?

    func stamp(at position: TimeInterval) -> Bool {
        guard let textView, let storage = textView.textStorage,
              let edit = LRC.timestampEdit(in: textView.string, caret: textView.selectedRange().location, at: position),
              textView.shouldChangeText(in: edit.range, replacementString: edit.replacement) else { return false }

        storage.replaceCharacters(in: edit.range, with: edit.replacement)
        textView.didChangeText()
        let caret = NSRange(location: edit.nextCaret, length: 0)
        textView.setSelectedRange(caret)
        textView.scrollRangeToVisible(caret)
        textView.window?.makeFirstResponder(textView)
        return true
    }
}

@MainActor
private struct LRCTextEditor: NSViewRepresentable {
    @Binding var text: String
    let editor: LRCEditorController

    func makeNSView(context: Context) -> NSScrollView {
        let scrollView = NSTextView.scrollableTextView()
        scrollView.borderType = .noBorder
        guard let textView = scrollView.documentView as? NSTextView else { return scrollView }

        textView.isRichText = false
        textView.importsGraphics = false
        textView.allowsUndo = true
        textView.isAutomaticQuoteSubstitutionEnabled = false
        textView.isAutomaticDashSubstitutionEnabled = false
        textView.isAutomaticTextReplacementEnabled = false
        textView.isContinuousSpellCheckingEnabled = false
        textView.font = .monospacedSystemFont(ofSize: 13, weight: .regular)
        textView.textContainerInset = NSSize(width: 6, height: 8)
        textView.setAccessibilityLabel("Songtext im LRC-Format")
        textView.string = text
        textView.delegate = context.coordinator
        editor.textView = textView
        return scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        context.coordinator.text = $text
        guard let textView = scrollView.documentView as? NSTextView, textView.string != text else { return }
        textView.string = text
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(text: $text)
    }

    @MainActor
    final class Coordinator: NSObject, NSTextViewDelegate {
        var text: Binding<String>

        init(text: Binding<String>) {
            self.text = text
        }

        func textDidChange(_ notification: Notification) {
            guard let view = notification.object as? NSTextView else { return }
            text.wrappedValue = view.string
        }
    }
}
