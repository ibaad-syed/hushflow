// Times Apple's on-device SpeechAnalyzer (macOS 26+) on WAV files.
// Usage: apple_stt <speech|dictation> <file.wav>...
import AVFoundation
import Foundation
import Speech

func makeModule(_ kind: String, _ locale: Locale) -> any SpeechModule {
    kind == "dictation"
        ? DictationTranscriber(locale: locale, preset: .shortDictation)
        : SpeechTranscriber(locale: locale, preset: .transcription)
}

func transcribe(kind: String, url: URL, locale: Locale) async throws -> (ms: Double, text: String) {
    let module = makeModule(kind, locale)
    let analyzer = SpeechAnalyzer(modules: [module])
    let file = try AVAudioFile(forReading: url)
    let start = Date()
    let collector = Task { () -> String in
        var text = ""
        if let t = module as? SpeechTranscriber {
            for try await r in t.results where r.isFinal { text += String(r.text.characters) }
        } else if let d = module as? DictationTranscriber {
            for try await r in d.results where r.isFinal { text += String(r.text.characters) }
        }
        return text
    }
    if let last = try await analyzer.analyzeSequence(from: file) {
        try await analyzer.finalizeAndFinish(through: last)
    } else {
        await analyzer.cancelAndFinishNow()
    }
    let text = try await collector.value
    return (Date().timeIntervalSince(start) * 1000, text)
}

@main
struct Main {
    static func main() async throws {
        let args = CommandLine.arguments
        let kind = args[1]
        guard SpeechTranscriber.isAvailable || kind == "dictation" else {
            print("SpeechTranscriber not available on this device"); return
        }
        let wanted = Locale(identifier: "en-US")
        let locale = await (kind == "dictation"
            ? DictationTranscriber.supportedLocale(equivalentTo: wanted)
            : SpeechTranscriber.supportedLocale(equivalentTo: wanted)) ?? wanted

        // Make sure the OS-managed model is installed (shared system asset, not bundled with the app).
        let probe = makeModule(kind, locale)
        let t0 = Date()
        if let req = try await AssetInventory.assetInstallationRequest(supporting: [probe]) {
            print("installing system speech assets...")
            try await req.downloadAndInstall()
        }
        print(String(format: "asset check/install: %.0f ms", Date().timeIntervalSince(t0) * 1000))

        for path in args.dropFirst(2) {
            let url = URL(fileURLWithPath: path)
            var times: [Double] = []
            var text = ""
            for _ in 0..<3 {
                let r = try await transcribe(kind: kind, url: url, locale: locale)
                times.append(r.ms); text = r.text
            }
            let ms = times.map { String(format: "%.0f", $0) }.joined(separator: ",")
            print("\(kind) \(url.lastPathComponent) runs=[\(ms)] ms | \(text.prefix(110))")
        }
    }
}
