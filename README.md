# brhap 

A (currently) macOS launcher for ArmA 3, targeted to mission and mod developers. It is early in development, with a future compatibility with linux-like platforms.

## build and run

```
cargo run -p brhap-app
```

## features
- [x] Detects steam libraries and the first ArmA 3 installation it can access.
- [x] Detects installed mods and has a means to discover dependencies
- [x] Allows overriding a mod path (to `.hemttout/dev`, for example)
- [x] Supports Intel or Apple Silicon (default)
- [x] Support for Steam Overlay
- [x] Basic launch profiles
- [ ] Detects and uses CDLC
- [ ] Detects and uses Contact

## action shots
<img width="1012" height="872" alt="image" src="https://github.com/user-attachments/assets/6839ee07-a7d2-4ee9-ab48-b5b0fbcdb78a" />
<img width="1012" height="872" alt="image" src="https://github.com/user-attachments/assets/c48c4dfa-0c3b-476a-a9f5-c5a238e2d681" />
<img width="1012" height="872" alt="image" src="https://github.com/user-attachments/assets/ea962d36-bded-410f-816f-2af188749c38" />
<img width="1012" height="872" alt="image" src="https://github.com/user-attachments/assets/ff35a03e-d181-4d68-88e6-8c433825ae49" />
<img width="1012" height="872" alt="image" src="https://github.com/user-attachments/assets/b6253e48-f60c-4cd1-af61-b150ff654d92" />

## verifying Linux release signatures

Linux AppImage releases are signed with a detached GPG signature (the `.asc`
file attached alongside each `.AppImage` on the release page).

Key fingerprint: `5577 6421 1B50 45E8 45E8  6ED7 F7B1 4592 0914 BEDB`

The public key is available from [keys.openpgp.org](https://keys.openpgp.org/search?q=development%40outof.coffee).

```
gpg --keyserver hkps://keys.openpgp.org --recv-keys 557764211B5045E845E86ED7F7B145920914BEDB
gpg --verify brhap-app_<version>_<arch>.AppImage.asc brhap-app_<version>_<arch>.AppImage
```
