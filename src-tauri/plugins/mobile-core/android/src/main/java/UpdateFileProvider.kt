package app.kouros.mobile

import androidx.core.content.FileProvider

/** Separate provider identity so manifest merging cannot widen Tauri's file provider. */
class UpdateFileProvider : FileProvider()
