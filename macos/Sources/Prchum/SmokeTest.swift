import Foundation
import PrchumKit

/// Headless verification that the shell and the core actually talk to each
/// other: a diff parses through the C boundary into typed Swift values, bad
/// input is rejected with a message instead of a crash, and an asynchronous
/// event makes it from a core thread back to the main queue.
///
/// Returns a process exit code: 0 on success.
@MainActor
func runSmokeTest() -> Int32 {
    print("prchum core \(Core.version)")

    let patch = """
        diff --git a/src/lib.rs b/src/lib.rs
        --- a/src/lib.rs
        +++ b/src/lib.rs
        @@ -1,3 +1,4 @@
         fn main() {
        -    old();
        +    new();
        +    extra();
         }
        diff --git a/gone.txt b/gone.txt
        deleted file mode 100644
        --- a/gone.txt
        +++ /dev/null
        @@ -1 +0,0 @@
        -bye
        """

    // Session round trip: parse in the core, read back typed values.
    do {
        let session = try CoreSession(title: "smoke", patch: patch)
        guard session.title == "smoke", session.fileCount == 2 else {
            print("FAIL: session shape: \(session.title), \(session.fileCount) files")
            return 1
        }
        let files = try session.files()
        guard files[0].displayPath == "src/lib.rs", files[0].status == .modified,
            files[1].displayPath == "gone.txt", files[1].status == .deleted
        else {
            print("FAIL: file identities: \(files.map(\.displayPath))")
            return 1
        }
        let counts = files[0].changeCounts
        guard counts.added == 2, counts.deleted == 1 else {
            print("FAIL: change counts: \(counts)")
            return 1
        }
        let lines = files[0].hunks[0].lines
        guard lines[0].kind == .context, lines[0].oldLine == 1, lines[0].newLine == 1,
            lines[1].kind == .deletion, lines[1].oldLine == 2, lines[1].newLine == nil,
            lines[2].kind == .addition, lines[2].newLine == 2,
            lines[2].patchPosition == 3
        else {
            print("FAIL: line model: \(lines)")
            return 1
        }
        print("session round trip ok (\(session.fileCount) files)")
    } catch {
        print("FAIL: session over a valid diff: \(error)")
        return 1
    }

    // Invalid input must be rejected with a message, not crash.
    do {
        _ = try CoreSession(title: "bad", patch: "this is not a diff")
        print("FAIL: core accepted garbage as a diff")
        return 1
    } catch let error as CoreError {
        guard !error.message.isEmpty else {
            print("FAIL: rejection carried no message")
            return 1
        }
        print("input validation ok (\(error.message))")
    } catch {
        print("FAIL: unexpected error type: \(error)")
        return 1
    }

    // Async event round trip: core dispatch thread → main queue.
    var receivedSequence: UInt64?
    let coreApp = CoreApp { event in
        if case let .pong(sequence) = event {
            receivedSequence = sequence
            CFRunLoopStop(CFRunLoopGetMain())
        }
    }
    coreApp.ping(sequence: 42)
    // Give the main run loop up to five seconds to receive the pong.
    let outcome = CFRunLoopRunInMode(.defaultMode, 5.0, false)
    guard receivedSequence == 42, outcome == .stopped else {
        print("FAIL: pong not delivered (got \(String(describing: receivedSequence)))")
        return 1
    }
    print("event round trip ok (pong 42)")

    print("smoke test passed")
    return 0
}
