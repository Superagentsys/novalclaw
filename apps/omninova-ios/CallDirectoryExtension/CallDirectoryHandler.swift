import CallKit
import Foundation

/// iOS Call Directory Extension。
/// App 主目标把最新黑名单写入 App Group 的 plist/JSON，本扩展读取后下发到系统。
final class CallDirectoryHandler: CXCallDirectoryProvider {
    override func beginRequest(with context: CXCallDirectoryExtensionContext) {
        context.delegate = self
        do {
            try addIdentificationEntries(to: context)
            try addBlockingEntries(to: context)
            context.completeRequest()
        } catch {
            context.cancelRequest(withError: error)
        }
    }

    private func addIdentificationEntries(to context: CXCallDirectoryExtensionContext) throws {
        let entries = loadIdentificationEntries()
        for (phone, label) in entries.sorted(by: { $0.key < $1.key }) {
            context.addIdentificationEntry(withNextSequentialPhoneNumber: phone, label: label)
        }
    }

    private func addBlockingEntries(to context: CXCallDirectoryExtensionContext) throws {
        let numbers = loadBlockedNumbers()
        for phone in numbers.sorted() {
            context.addBlockingEntry(withNextSequentialPhoneNumber: phone)
        }
    }

    private func loadIdentificationEntries() -> [CXCallDirectoryPhoneNumber: String] {
        guard let url = sharedDirectoryURL()?.appendingPathComponent("identification.json"),
              let data = try? Data(contentsOf: url),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: String] else {
            return [:]
        }
        var result: [CXCallDirectoryPhoneNumber: String] = [:]
        for (k, v) in obj {
            if let num = CXCallDirectoryPhoneNumber(k) {
                result[num] = v
            }
        }
        return result
    }

    private func loadBlockedNumbers() -> [CXCallDirectoryPhoneNumber] {
        guard let url = sharedDirectoryURL()?.appendingPathComponent("blocked.json"),
              let data = try? Data(contentsOf: url),
              let arr = try? JSONSerialization.jsonObject(with: data) as? [String] else {
            return []
        }
        return arr.compactMap { CXCallDirectoryPhoneNumber($0) }
    }

    private func sharedDirectoryURL() -> URL? {
        FileManager.default.containerURL(
            forSecurityApplicationGroupIdentifier: "group.com.omninova.phoneagent"
        )
    }
}

extension CallDirectoryHandler: CXCallDirectoryExtensionContextDelegate {
    func requestFailed(for extensionContext: CXCallDirectoryExtensionContext, withError error: Error) {
        NSLog("[CallDirectory] request failed: \(error)")
    }
}
