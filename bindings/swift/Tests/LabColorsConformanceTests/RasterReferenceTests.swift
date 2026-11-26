import Foundation
import XCTest
import LabColors

final class RasterReferenceTests: XCTestCase {
    func testRestorationRequiresBothPixelsFromTheCapturedField() throws {
        let pixels = Data([20, 40, 60, 255, 1, 120, 200, 255])
        var changed = pixels
        changed[6] += 1

        func start(with ordinary: Data) throws -> RasterChoice {
            let reference = try XCTUnwrap(captureRasterReference(width: 2, height: 1, rgba: pixels))
            return reference.start(width: 2, height: 1, rgba: ordinary, hasPaints: true)
        }

        XCTAssertEqual(try start(with: pixels).action(), "preserved")
        let restored = try start(with: changed)
        XCTAssertEqual(restored.action(), "try-inverse")
        restored.inspect(width: 2, height: 1, rgba: changed)
        XCTAssertEqual(restored.action(), "try-visible")
        restored.inspect(width: 2, height: 1, rgba: pixels)
        XCTAssertEqual(restored.action(), "restored")

        let different = try start(with: changed)
        different.refused(searchLimit: false)
        different.inspect(width: 2, height: 1, rgba: changed)
        XCTAssertEqual(different.action(), "different-bitmap")

        let limited = try start(with: changed)
        limited.refused(searchLimit: true)
        limited.inspect(width: 2, height: 1, rgba: changed)
        XCTAssertEqual(limited.action(), "search-limit")

        let oversized = try start(with: changed)
        oversized.proposalLimit()
        oversized.inspect(width: 2, height: 1, rgba: changed)
        XCTAssertEqual(oversized.action(), "proposal-limit")
    }

    func testPaintProposalReturnsPackedRGBOrNil() {
        let backdrop = Data([0, 159, 255])
        XCTAssertEqual(paintProposal(desired: Data([1, 120, 200]), backdrop: backdrop, alpha: 0.5), 0x02_51_91)
        XCTAssertNil(paintProposal(desired: Data([1, 120, 0]), backdrop: backdrop, alpha: 0.5))
    }
}
