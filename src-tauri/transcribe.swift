import Speech
import AVFoundation

var currentTranscription = ""
var shouldStop = false

// On SIGINT: set flag so the main loop exits cleanly (letting recognizer finalize)
signal(SIGINT) { _ in
    shouldStop = true
}

print("Starting...")

let semaphore = DispatchSemaphore(value: 0)
SFSpeechRecognizer.requestAuthorization { status in
    semaphore.signal()
}
semaphore.wait()

guard let recognizer = SFSpeechRecognizer(locale: Locale(identifier: "en-US")),
      recognizer.isAvailable else {
    print("ERROR: Speech recognizer not available")
    exit(1)
}

let audioEngine = AVAudioEngine()
let request = SFSpeechAudioBufferRecognitionRequest()
request.shouldReportPartialResults = true

var lastSoundTime = Date()
let silenceThreshold: Float = -45.0
let silenceDuration: TimeInterval = 1.5
let minRecordingTime: TimeInterval = 0.5

let inputNode = audioEngine.inputNode
let format = inputNode.outputFormat(forBus: 0)

inputNode.installTap(onBus: 0, bufferSize: 1024, format: format) { buffer, _ in
    request.append(buffer)

    guard let channelData = buffer.floatChannelData?[0] else { return }
    let frameLength = Int(buffer.frameLength)
    var sum: Float = 0
    for i in 0..<frameLength {
        sum += channelData[i] * channelData[i]
    }
    let rms = sqrt(sum / Float(frameLength))
    let db = 20 * log10(rms)

    if db > silenceThreshold {
        lastSoundTime = Date()
    }
}

audioEngine.prepare()
try! audioEngine.start()
print("Audio engine started — speak now!")

var done = false

recognizer.recognitionTask(with: request) { result, error in
    if let result = result {
        currentTranscription = result.bestTranscription.formattedString
        if result.isFinal { done = true }
    } else if let error = error {
        print("ERROR: \(error.localizedDescription)")
        done = true
    }
}

let startTime = Date()
while true {
    RunLoop.current.run(until: Date().addingTimeInterval(0.1))
    let elapsed = Date().timeIntervalSince(startTime)
    let silenceElapsed = Date().timeIntervalSince(lastSoundTime)

    if shouldStop { break }
    if elapsed > minRecordingTime && silenceElapsed > silenceDuration { break }
    if elapsed > 30 { break }
}

audioEngine.stop()
inputNode.removeTap(onBus: 0)
request.endAudio()

// Give the recognizer time to finalize the last words
let waitDeadline = Date().addingTimeInterval(3)
while !done && Date() < waitDeadline {
    RunLoop.current.run(until: Date().addingTimeInterval(0.1))
}

print("RESULT: \(currentTranscription)")
