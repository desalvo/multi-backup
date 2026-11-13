// macOS Touch ID adapter. Local Keychain stores only the vault unlock password.
// It is device-only, protected by current enrolled biometrics. No cloud sync.
// Requires a user opt-in after successful password unlock.
import Foundation
import Security
import LocalAuthentication

let service = "io.github.desalvo.multi-backup.biometric.v1"
let account = "default-age-vault"
let args = CommandLine.arguments
func fail(_ message: String, _ code: Int32 = 1) -> Never {
    FileHandle.standardError.write(Data((message + "\n").utf8))
    exit(code)
}
func baseQuery() -> [CFString: Any] {
    return [
        kSecClass: kSecClassGenericPassword,
        kSecAttrService: service,
        kSecAttrAccount: account
    ]
}
let command = args.count > 1 ? args[1] : "status"
let context = LAContext()
var error: NSError?
let canUseBiometrics = context.canEvaluatePolicy(
    .deviceOwnerAuthenticationWithBiometrics, error: &error)
switch command {
case "status":
    if canUseBiometrics {
        FileHandle.standardOutput.write(Data("available\n".utf8))
        exit(0)
    }
    fail("Touch ID is not available or enrolled on this Mac.", 2)
case "enroll":
    guard canUseBiometrics else { fail("Touch ID not available or enrolled", 2) }
    let data = FileHandle.standardInput.readDataToEndOfFile()
    guard data.count >= 12 && data.count <= 1024 else {
        fail("Invalid vault unlock credential")
    }
    var secError: Unmanaged<CFError>?
    guard let access = SecAccessControlCreateWithFlags(
        nil, kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
        .biometryCurrentSet, &secError) else {
        fail("Could not create biometric Keychain access control")
    }
    var query = baseQuery()
    query[kSecAttrAccessControl] = access
    query[kSecValueData] = data
    query[kSecUseAuthenticationContext] = context
    let status = SecItemAdd(query as CFDictionary, nil)
    guard status == errSecSuccess else { fail("Keychain enrollment failed (status \(status))") }
    FileHandle.standardOutput.write(Data("enrolled\n".utf8))
case "unlock":
    guard canUseBiometrics else { fail("Touch ID unavailable", 2) }
    var query = baseQuery()
    query[kSecMatchLimit] = kSecMatchLimitOne
    query[kSecReturnData] = kCFBooleanTrue!
    query[kSecUseAuthenticationContext] = context
    query[kSecUseOperationPrompt] = "Unlock Multi Backup encryption key with Touch ID"
    var result: CFTypeRef?
    let status = SecItemCopyMatching(query as CFDictionary, &result)
    guard status == errSecSuccess, let secret = result as? Data else {
        fail("Touch ID unlock denied or not enrolled (status \(status))", 3)
    }
    // The password is returned ONLY over stdout to the calling app's pipe.
    FileHandle.standardOutput.write(secret)
case "disable":
    let status = SecItemDelete(baseQuery() as CFDictionary)
    guard status == errSecSuccess || status == errSecItemNotFound else {
        fail("Keychain biometric removal failed (status \(status))")
    }
    FileHandle.standardOutput.write(Data("disabled\n".utf8))
default:
    fail("Unknown command; use status, enroll, unlock or disable")
}
