import Foundation

// Stub used when the macOS SDK predates SpeechAnalyzer (macOS 26). Every entry
// point reports the engine as unavailable so Hushflow falls back to its own models.

private func unavailable() -> UnsafeMutablePointer<AppleSpeechResult> {
    let ptr = UnsafeMutablePointer<AppleSpeechResult>.allocate(capacity: 1)
    ptr.initialize(
        to: AppleSpeechResult(
            text: nil, success: 0,
            error_message: strdup("Apple speech recognition is not available in this build.")))
    return ptr
}

@_cdecl("apple_speech_available")
public func appleSpeechAvailable() -> Int32 { 0 }

@_cdecl("apple_speech_prepare")
public func appleSpeechPrepare() -> UnsafeMutablePointer<AppleSpeechResult> { unavailable() }

@_cdecl("apple_speech_transcribe")
public func appleSpeechTranscribe(
    _ samples: UnsafePointer<Float>?, _ count: Int
) -> UnsafeMutablePointer<AppleSpeechResult> { unavailable() }

@_cdecl("apple_speech_stream_begin")
public func appleSpeechStreamBegin(
    _ context: UnsafeMutableRawPointer?, _ callback: AppleSpeechStreamCallback?
) -> UnsafeMutableRawPointer? { nil }

@_cdecl("apple_speech_stream_feed")
public func appleSpeechStreamFeed(
    _ handle: UnsafeMutableRawPointer?, _ samples: UnsafePointer<Float>?, _ count: Int
) {}

@_cdecl("apple_speech_stream_finish")
public func appleSpeechStreamFinish(
    _ handle: UnsafeMutableRawPointer?
) -> UnsafeMutablePointer<AppleSpeechResult> { unavailable() }

@_cdecl("apple_speech_stream_cancel")
public func appleSpeechStreamCancel(_ handle: UnsafeMutableRawPointer?) {}

@_cdecl("apple_speech_free_result")
public func appleSpeechFreeResult(_ result: UnsafeMutablePointer<AppleSpeechResult>?) {
    guard let result else { return }
    free(result.pointee.text)
    free(result.pointee.error_message)
    result.deinitialize(count: 1)
    result.deallocate()
}
