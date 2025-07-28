# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- GitHub Actions workflow for automated releases
- CI workflow for testing and cross-compilation checks
- Multi-architecture Linux binary builds (x86_64 glibc/musl, ARM64)
- SHA256 checksums for release verification
- Parallel model loading during audio recording for improved performance
- Interactive tuning mode with `--tune-interactive`
- Special token filtering for cleaner Whisper transcription output

### Changed
- Improved silence detection tuning algorithm with better suggestions
- Model preparation now happens in parallel with audio recording
- Enhanced error handling and user feedback

### Fixed
- Duplicate model loading eliminated through parallel processing optimization
- Unwanted tokens (like `[BLANK_AUDIO]`) now filtered from transcription output

## [0.1.0] - Initial Release

### Added
- Local Whisper transcription with automatic model downloading
- OpenAI Whisper API support for cloud transcription
- Wayland-native clipboard integration
- PipeWire audio recording with intelligent silence detection
- Configurable audio thresholds with auto-tuning
- LLM text refinement with multiple profiles
- Multiple output options (clipboard, stdout, auto-paste)
- XDG-compliant configuration management
- Comprehensive logging system
- Interactive and non-interactive modes 