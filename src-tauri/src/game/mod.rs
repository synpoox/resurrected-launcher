//! Game module: detect or select D2R, assess game/loader and extension readiness,
//! launch an explicit product and mode, and track running processes.
//!
//! Returns actionable readiness reasons rather than spreading checks into React.
//! Slice 3; nothing here touches a real D2R installation before the slice 2 gate (#39).
