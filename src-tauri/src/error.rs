use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClearMetaError {
    #[error("不支持的文件格式: {0}")]
    UnsupportedFormat(String),

    #[error("文件不存在: {0}")]
    FileNotFound(String),

    #[error("PDF 处理失败: {0}")]
    Pdf(String),

    #[error("Office 处理失败: {0}")]
    Office(String),

    #[error("图片处理失败: {0}")]
    Image(String),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("ZIP 错误: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("路径超出允许范围: {0}")]
    OutOfScope(String),

    #[error("文件过大: {0}")]
    TooLarge(String),

    #[error("文件结构损坏: {0}")]
    Corrupted(String),

    #[error("文件正被其他程序占用，无法覆盖: {0}。请先关闭正在使用该文件的程序（预览、编辑器等）后重试")]
    FileLocked(String),

    #[error("疑似压缩包炸弹: {0}")]
    ArchiveBomb(String),

    #[error("时间格式无法识别：{0}。请按 2026-09-14 09:50:53 这样的格式填写")]
    InvalidDate(String),

    #[error("内部状态错误: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, ClearMetaError>;
