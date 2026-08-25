import CPrchum
import Foundation

/// An event from the core, typed for the shell.
public enum CoreEvent {
    /// Reply to `ping(sequence:)`; proves the async round trip.
    case pong(sequence: UInt64)
}

/// Root handle for a core instance and its event channel.
///
/// Events are delivered on the core's single dispatch thread and hopped to
/// the main actor here, in order, before `onEvent` sees them.
public final class CoreApp {
    /// Holds the delivery closure so the C callback can reach it through
    /// the userdata pointer.
    private final class EventSink {
        let deliver: (CoreEvent) -> Void

        init(deliver: @escaping (CoreEvent) -> Void) {
            self.deliver = deliver
        }
    }

    private var handle: OpaquePointer?
    private let sink: EventSink

    public init(onEvent: @escaping @MainActor (CoreEvent) -> Void) {
        sink = EventSink { event in
            // Dispatch (not Task) keeps strict event order.
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    onEvent(event)
                }
            }
        }

        // A capture-free C function; context arrives through userdata.
        let callback: @convention(c) (UnsafePointer<PcEvent>?, UnsafeMutableRawPointer?) -> Void = {
            event, userdata in
            guard let event = event?.pointee, let userdata else { return }
            // Unretained is safe: pc_app_free joins the dispatch thread in
            // deinit before the sink is released.
            let sink = Unmanaged<EventSink>.fromOpaque(userdata).takeUnretainedValue()
            switch event.kind {
            case UInt32(PC_EVENT_PONG):
                sink.deliver(.pong(sequence: event.seq))
            default:
                // Unknown kinds are forward compatibility, not an error.
                break
            }
        }
        handle = pc_app_new(callback, Unmanaged.passUnretained(sink).toOpaque())
    }

    deinit {
        pc_app_free(handle)
    }

    /// Asks the core to answer with `.pong` from a worker thread.
    public func ping(sequence: UInt64) {
        pc_app_ping(handle, sequence)
    }
}
