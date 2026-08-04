//! Доменные модели проекта. В skeleton — только учебный экспонат `Demo`
//! (эталон admin-CRUD доменной сущности). Реальные проекты добавляют свои
//! (`Order`, `Category`, ...) рядом и удаляют `demo`.

pub mod demo;

pub use demo::{Demo, DemoListFilter};
