# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v.0.1.5] - 2025-09-25

### Added
- `ExtendedMemoryMap` with 32 bits on first leverl, 64 on second, 64 on third

### Changed
- Removed magic constant for thid index calculation in standart_memory_map, max_memory_map

## [v0.1.4] - 2025-07-28

### Added

- `alloc_at()` method for allocation by specific index
- Implemented **PartialEq** for Maps
- Changed MemoryMap size behavior

## [v0.1.3] - 2025-07-14

### Added

- `is_allocated()` method for all memory map types to check allocation status of specific indices
- `reset()` method for all memory map types to clear all allocations and return to initial state

## [v0.1.2] - 2025-07-04

### Changed

- Bump solana version to 2.x.x

## [v0.1.1] - 2025-05-13

### Added

- Fabric constructor for all types of maps
- Error-handling with typed errors
- Unit-tests for all types of maps

### Changed

- Standard memory map -> Max memory map
- Trade memory map -> Standard memory map
- Dynamic dispatch -> Static dispatch

## [v0.1.0] - 2025-04-28

### Added

- Unsafe lib implementation
