# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [3.0.0] - 2022-01-10
### Added
- Keep a changelog
### Changed
- Support laptops with AMD dGPU
  + `hybrid`, `integrated`, `vfio` only
  + Modes unsupported by AMD dGPU will return an error
- `nvidia` mode is now `dedicated`
- Don't write the config twice on laptops with hard-mux switch
- CLI print zbus error string if available
- Heavy internal cleanup and refactor to make the project a bit nicer to work with