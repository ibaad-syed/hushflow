#ifndef apple_speech_bridge_h
#define apple_speech_bridge_h

// C-compatible declarations for the Apple SpeechAnalyzer (macOS 26+) bridge.
// Every function blocks the calling thread; never call from the main thread.

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    char* text;          // Transcript (success = 1); may be empty
    int success;         // 0 for failure, 1 for success
    char* error_message; // Only valid when success = 0
} AppleSpeechResult;

// Live text callback: committed = finalized text so far, tentative = current
// volatile hypothesis. Called from a background thread.
typedef void (*AppleSpeechStreamCallback)(void* context, const char* committed, const char* tentative);

// 1 when SpeechTranscriber can run here (macOS 26+, supported hardware, en-US locale).
int apple_speech_available(void);

// Install the OS-managed speech assets if needed. Can take a while on first use.
AppleSpeechResult* apple_speech_prepare(void);

// Transcribe 16 kHz mono float samples in one pass.
AppleSpeechResult* apple_speech_transcribe(const float* samples, long count);

// Streaming session. `context` is passed back to `callback` untouched.
void* apple_speech_stream_begin(void* context, AppleSpeechStreamCallback callback);
void apple_speech_stream_feed(void* session, const float* samples, long count);
// Finalizes and releases the session; returns the full transcript.
AppleSpeechResult* apple_speech_stream_finish(void* session);
// Abandons and releases the session.
void apple_speech_stream_cancel(void* session);

void apple_speech_free_result(AppleSpeechResult* result);

#ifdef __cplusplus
}
#endif

#endif /* apple_speech_bridge_h */
