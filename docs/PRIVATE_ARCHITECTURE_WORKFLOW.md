# Confidential architecture workflow

Multi Backup is a PUBLIC GitHub repository. Treat GitHub history as worldwide
downloadable. Only an operator-chosen AES-256 R6 password-encrypted PDF may be
committed at docs/private/architecture-encrypted.pdf. Do not commit plaintext
PDFs, diagrams, source scripts, hostnames, network topology, IPs, keys, or passwords.

Use the separate offline architecture kit supplied to the operator. It draws
the private PDF in memory, prompts for a password locally using getpass,
encrypts the document with AES-256, and writes only the encrypted PDF.
Nothing containing the private diagram source is included in this public repo.

1. On your computer install reportlab>=4,<5 and PyMuPDF>=1.24,<2.
2. Run the offline generator. Choose a strong unique 20-40 UTF-8 byte password.
   Do NOT type your password into chat, command arguments, Git or CI.
3. Verify the encrypted PDF opens with the password; keep an offline copy.
4. Optional public commit: copy ONLY the encrypted output to
   docs/private/architecture-encrypted.pdf; install pypdf>=5,<7 locally;
   run python scripts/check-confidential-artifacts.py and inspect git diff.
5. Commit only after confirming no private plaintext source is staged.

Preferred: store even the ciphertext in a separate GitHub PRIVATE repository
or your encrypted offline storage; a strong PDF password is required against
public offline guessing and Git history cannot be reliably erased.

The public CI gate rejects other docs/private paths and PDFs without strong
AES-256 R6 encryption. No password is ever requested or stored by the gate.
Future chats can continue from a document decrypted locally and selectively
provided to that chat; unreadable ciphertext alone does not provide handoff.

Debian 12 Bookworm and Debian 13 Trixie are both supported as native Rust
DEB preview targets, for AMD64 and ARM64. Packages remain prerelease/unsigned
until real install, update, signing and restore certification succeeds.
