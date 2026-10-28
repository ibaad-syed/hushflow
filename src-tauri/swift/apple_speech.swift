import AVFAudio
import Dispatch
import Foundation
import Speech

// MARK: - Apple SpeechAnalyzer bridge (macOS 26+)
// Compiled via the Cargo build script for Apple Silicon targets. The OS owns the
// speech model (installed through AssetInventory), so nothing ships in the app.

private typealias ResultPointer = UnsafeMutablePointer<AppleSpeechResult>

/// Hushflow captures 16 kHz mono Float32; SpeechAnalyzer may want something else.
private let inputFormat = AVAudioFormat(
    commonFormat: .pcmFormatFloat32, sampleRate: 16_000, channels: 1, interleaved: false)!

private func duplicateCString(_ text: String) -> UnsafeMutablePointer<CChar>? {
    text.withCString { strdup($0) }
}

private func makeResult(text: String? = nil, error: String? = nil) -> ResultPointer {
    let ptr = ResultPointer.allocate(capacity: 1)
    ptr.initialize(to: AppleSpeechResult(text: nil, success: 0, error_message: nil))
    if let error {
        ptr.pointee.error_message = duplicateCString(error)
    } else {
        ptr.pointee.text = duplicateCString(text ?? "")
        ptr.pointee.success = 1
    }
    return ptr
}

/// Runs an async operation to completion on a background task, blocking the caller.
private func blocking<T>(_ operation: @escaping @Sendable () async throws -> T) -> Result<T, Error> {
    let semaphore = DispatchSemaphore(value: 0)
    nonisolated(unsafe) var outcome: Result<T, Error>!
    Task.detached {
        do { outcome = .success(try await operation()) } catch { outcome = .failure(error) }
        semaphore.signal()
    }
    semaphore.wait()
    return outcome
}

@available(macOS 26.0, *)
private enum Engine {
    static let wantedLocale = Locale(identifier: "en-US")

    static func locale() async -> Locale? {
        await SpeechTranscriber.supportedLocale(equivalentTo: wantedLocale)
    }

    static func makeTranscriber(_ locale: Locale, live: Bool) -> SpeechTranscriber {
        SpeechTranscriber(
            locale: locale,
            transcriptionOptions: [],
            reportingOptions: live ? [.volatileResults, .fastResults] : [],
            attributeOptions: [])
    }

    static func prepare() async throws {
        guard let locale = await locale() else {
            throw BridgeError("English speech recognition is not supported on this Mac.")
        }
        let module = makeTranscriber(locale, live: false)
        if let request = try await AssetInventory.assetInstallationRequest(supporting: [module]) {
            try await request.downloadAndInstall()
        }
    }

    /// Converts Hushflow's 16 kHz mono samples into the analyzer's preferred format.
    static func buffer(
        _ samples: UnsafeBufferPointer<Float>, to format: AVAudioFormat,
        converter: AVAudioConverter?
    ) throws -> AVAudioPCMBuffer {
        let frames = AVAudioFrameCount(samples.count)
        guard let source = AVAudioPCMBuffer(pcmFormat: inputFormat, frameCapacity: frames) else {
            throw BridgeError("Could not allocate an audio buffer.")
        }
        source.frameLength = frames
        if let base = samples.baseAddress {
            source.floatChannelData![0].update(from: base, count: samples.count)
        }
        guard let converter else { return source }

        let ratio = format.sampleRate / inputFormat.sampleRate
        let capacity = AVAudioFrameCount((Double(frames) * ratio).rounded(.up)) + 1
        guard let output = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: capacity) else {
            throw BridgeError("Could not allocate a converted audio buffer.")
        }
        nonisolated(unsafe) var consumed = false
        var conversionError: NSError?
        converter.convert(to: output, error: &conversionError) { _, status in
            if consumed {
                status.pointee = .noDataNow
                return nil
            }
            consumed = true
            status.pointee = .haveData
            return source
        }
        if let conversionError { throw conversionError }
        return output
    }

    static func converter(for modules: [any SpeechModule]) async -> (AVAudioFormat, AVAudioConverter?) {
        guard
            let best = await SpeechAnalyzer.bestAvailableAudioFormat(
                compatibleWith: modules, considering: inputFormat),
            best != inputFormat
        else { return (inputFormat, nil) }
        return (best, AVAudioConverter(from: inputFormat, to: best))
    }
}

private struct BridgeError: LocalizedError {
    let errorDescription: String?
    init(_ message: String) { errorDescription = message }
}

// MARK: - Availability and setup

@_cdecl("apple_speech_available")
public func appleSpeechAvailable() -> Int32 {
    guard #available(macOS 26.0, *) else { return 0 }
    guard SpeechTranscriber.isAvailable else { return 0 }
    let supported = blocking { await Engine.locale() != nil }
    return (try? supported.get()) == true ? 1 : 0
}

@_cdecl("apple_speech_prepare")
public func appleSpeechPrepare() -> UnsafeMutablePointer<AppleSpeechResult> {
    guard #available(macOS 26.0, *) else {
        return makeResult(error: "Apple speech recognition requires macOS 26 or newer.")
    }
    switch blocking({ try await Engine.prepare() }) {
    case .success: return makeResult(text: "")
    case .failure(let error): return makeResult(error: error.localizedDescription)
    }
}

// MARK: - Batch

@_cdecl("apple_speech_transcribe")
public func appleSpeechTranscribe(
    _ samples: UnsafePointer<Float>?, _ count: Int
) -> UnsafeMutablePointer<AppleSpeechResult> {
    guard #available(macOS 26.0, *) else {
        return makeResult(error: "Apple speech recognition requires macOS 26 or newer.")
    }
    // Copy before going async: the caller's buffer is only valid for this call.
    let audio = samples.map { Array(UnsafeBufferPointer(start: $0, count: count)) } ?? []

    let outcome = blocking { () -> String in
        guard let locale = await Engine.locale() else {
            throw BridgeError("English speech recognition is not supported on this Mac.")
        }
        let transcriber = Engine.makeTranscriber(locale, live: false)
        let (format, converter) = await Engine.converter(for: [transcriber])
        let buffer = try audio.withUnsafeBufferPointer {
            try Engine.buffer($0, to: format, converter: converter)
        }

        let collector = Task { () -> String in
            var text = ""
            for try await result in transcriber.results where result.isFinal {
                text += String(result.text.characters)
            }
            return text
        }
        let analyzer = SpeechAnalyzer(modules: [transcriber])
        let (input, continuation) = AsyncStream<AnalyzerInput>.makeStream()
        continuation.yield(AnalyzerInput(buffer: buffer))
        continuation.finish()
        if let last = try await analyzer.analyzeSequence(input) {
            try await analyzer.finalizeAndFinish(through: last)
        } else {
            await analyzer.cancelAndFinishNow()
        }
        return try await collector.value.trimmingCharacters(in: .whitespacesAndNewlines)
    }
    switch outcome {
    case .success(let text): return makeResult(text: text)
    case .failure(let error): return makeResult(error: error.localizedDescription)
    }
}

// MARK: - Streaming

@available(macOS 26.0, *)
private final class StreamSession: @unchecked Sendable {
    let analyzer: SpeechAnalyzer
    let continuation: AsyncStream<AnalyzerInput>.Continuation
    let format: AVAudioFormat
    let converter: AVAudioConverter?
    var results: Task<String, Error>?
    /// Feed calls arrive on whatever thread Hushflow's recorder uses; keep them ordered.
    let feedQueue = DispatchQueue(label: "apple-speech.feed")

    init(
        analyzer: SpeechAnalyzer, continuation: AsyncStream<AnalyzerInput>.Continuation,
        format: AVAudioFormat, converter: AVAudioConverter?
    ) {
        self.analyzer = analyzer
        self.continuation = continuation
        self.format = format
        self.converter = converter
    }
}

@_cdecl("apple_speech_stream_begin")
public func appleSpeechStreamBegin(
    _ context: UnsafeMutableRawPointer?, _ callback: AppleSpeechStreamCallback?
) -> UnsafeMutableRawPointer? {
    guard #available(macOS 26.0, *) else { return nil }
    nonisolated(unsafe) let context = context

    let outcome = blocking { () -> StreamSession in
        guard let locale = await Engine.locale() else {
            throw BridgeError("English speech recognition is not supported on this Mac.")
        }
        let transcriber = Engine.makeTranscriber(locale, live: true)
        let (format, converter) = await Engine.converter(for: [transcriber])
        let (input, continuation) = AsyncStream<AnalyzerInput>.makeStream()
        let analyzer = SpeechAnalyzer(modules: [transcriber])
        try await analyzer.prepareToAnalyze(in: format)
        try await analyzer.start(inputSequence: input)

        let session = StreamSession(
            analyzer: analyzer, continuation: continuation, format: format, converter: converter)
        session.results = Task {
            var committed = ""
            for try await result in transcriber.results {
                let text = String(result.text.characters)
                if result.isFinal {
                    committed += text
                    callback?(context, committed, "")
                } else {
                    callback?(context, committed, text)
                }
            }
            return committed
        }
        return session
    }
    guard case .success(let session) = outcome else { return nil }
    return Unmanaged.passRetained(session).toOpaque()
}

@_cdecl("apple_speech_stream_feed")
public func appleSpeechStreamFeed(
    _ handle: UnsafeMutableRawPointer?, _ samples: UnsafePointer<Float>?, _ count: Int
) {
    guard #available(macOS 26.0, *), let handle, let samples, count > 0 else { return }
    let session = Unmanaged<StreamSession>.fromOpaque(handle).takeUnretainedValue()
    let audio = Array(UnsafeBufferPointer(start: samples, count: count))
    session.feedQueue.sync {
        guard
            let buffer = try? audio.withUnsafeBufferPointer({
                try Engine.buffer($0, to: session.format, converter: session.converter)
            })
        else { return }
        session.continuation.yield(AnalyzerInput(buffer: buffer))
    }
}

@_cdecl("apple_speech_stream_finish")
public func appleSpeechStreamFinish(
    _ handle: UnsafeMutableRawPointer?
) -> UnsafeMutablePointer<AppleSpeechResult> {
    guard #available(macOS 26.0, *), let handle else {
        return makeResult(error: "No Apple speech session to finish.")
    }
    let session = Unmanaged<StreamSession>.fromOpaque(handle).takeRetainedValue()
    session.feedQueue.sync { session.continuation.finish() }
    let outcome = blocking { () -> String in
        try await session.analyzer.finalizeAndFinishThroughEndOfInput()
        let text = try await session.results?.value ?? ""
        return text.trimmingCharacters(in: .whitespacesAndNewlines)
    }
    switch outcome {
    case .success(let text): return makeResult(text: text)
    case .failure(let error): return makeResult(error: error.localizedDescription)
    }
}

@_cdecl("apple_speech_stream_cancel")
public func appleSpeechStreamCancel(_ handle: UnsafeMutableRawPointer?) {
    guard #available(macOS 26.0, *), let handle else { return }
    let session = Unmanaged<StreamSession>.fromOpaque(handle).takeRetainedValue()
    session.feedQueue.sync { session.continuation.finish() }
    _ = blocking {
        await session.analyzer.cancelAndFinishNow()
        session.results?.cancel()
        // Wait it out so the callback never fires after the caller frees its context.
        _ = try? await session.results?.value
    }
}

@_cdecl("apple_speech_free_result")
public func appleSpeechFreeResult(_ result: UnsafeMutablePointer<AppleSpeechResult>?) {
    guard let result else { return }
    free(result.pointee.text)
    free(result.pointee.error_message)
    result.deinitialize(count: 1)
    result.deallocate()
}
