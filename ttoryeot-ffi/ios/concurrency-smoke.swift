import Foundation
import TtoryeotSwift

@MainActor
func renderInBackground(_ document: Data, fonts: [Data]) async throws -> Ttoryeot.Output {
    try await Task.detached {
        try Ttoryeot.render(data: document, fonts: fonts)
    }.value
}
