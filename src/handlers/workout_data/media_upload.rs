use actix_multipart::{form::tempfile::TempFile, form::MultipartForm};
use actix_web::{web, HttpResponse};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;
use sha2::{Sha256, Digest};
use std::io::Read;

use crate::middleware::auth::Claims;
use crate::models::common::ApiResponse;

#[derive(Debug, MultipartForm)]
pub struct MediaUploadForm {
    #[multipart(limit = "50MB")]
    pub file: TempFile,
    pub workout_id: Option<actix_multipart::form::text::Text<String>>,
}

#[derive(serde::Serialize)]
pub struct MediaUploadResponse {
    pub file_url: String,
    pub file_type: String,
    pub file_size: u64,
    pub file_hash: String,
}

const UPLOAD_DIR: &str = "./uploads/workout_media";
const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024; // 50MB
const ALLOWED_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "mp4", "mov", "avi"];

#[tracing::instrument(
    name = "Upload workout media",
    skip(form, claims),
    fields(
        username = %claims.username,
        file_name = %form.file.file_name.as_deref().unwrap_or("unknown")
    )
)]
pub async fn upload_workout_media(
    MultipartForm(form): MultipartForm<MediaUploadForm>,
    claims: web::ReqData<Claims>,
) -> HttpResponse {
    tracing::info!("📎 Processing media upload for user: {}", claims.username);

    let user_id = match Uuid::parse_str(&claims.sub) {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("Failed to parse user ID: {}", e);
            return HttpResponse::InternalServerError().json(
                ApiResponse::<()>::error("Invalid user ID")
            );
        }
    };

    // Log file info for debugging
    tracing::info!("📎 File upload details - Name: {:?}, Size: {:?}, Content-Type: {:?}", 
        form.file.file_name, 
        form.file.size, 
        form.file.content_type
    );

    // Validate file
    if let Err(error_response) = validate_file(&form.file) {
        return error_response;
    }

    // Create upload directory if it doesn't exist
    if let Err(e) = fs::create_dir_all(UPLOAD_DIR) {
        tracing::error!("Failed to create upload directory: {}", e);
        return HttpResponse::InternalServerError().json(
            ApiResponse::<()>::error("Failed to create upload directory")
        );
    }

    // Generate unique filename with proper extension
    let original_filename = form.file.file_name.as_deref().unwrap_or("unknown");
    let mut extension = get_file_extension(original_filename);
    
    // If no extension or invalid extension, try to determine from content type
    if extension.is_empty() || !ALLOWED_EXTENSIONS.contains(&extension.as_str()) {
        if let Some(content_type) = &form.file.content_type {
            let mime_str = content_type.to_string();
            extension = match mime_str.as_str() {
                "image/jpeg" | "image/jpg" => "jpg".to_string(),
                "image/png" => "png".to_string(),
                "image/gif" => "gif".to_string(),
                "video/mp4" => "mp4".to_string(),
                "video/quicktime" => "mov".to_string(),
                "video/avi" | "video/x-msvideo" => "avi".to_string(),
                _ => extension, // Keep original if can't determine
            };
        }
    }
    
    // Final fallback
    if extension.is_empty() {
        extension = "png".to_string(); // Default to png for images
    }
    
    let unique_filename = format!("{}_{}.{}", user_id, Uuid::new_v4(), extension);
    let file_path = PathBuf::from(UPLOAD_DIR).join(&unique_filename);

    // Calculate file hash for integrity checking
    let file_hash = match calculate_file_hash(&form.file.file.path()).await {
        Ok(hash) => hash,
        Err(e) => {
            tracing::error!("Failed to calculate file hash: {}", e);
            return HttpResponse::InternalServerError().json(
                ApiResponse::<()>::error("Failed to process file")
            );
        }
    };

    // Copy the uploaded file to permanent location (using copy instead of persist to handle cross-device links)
    match std::fs::copy(form.file.file.path(), &file_path) {
        Ok(_) => {
            tracing::info!("✅ Successfully saved file: {}", file_path.display());
            
            // Get file metadata
            let file_size = match fs::metadata(&file_path) {
                Ok(metadata) => metadata.len(),
                Err(_) => 0,
            };

            let file_url = format!("/api/workout-media/{}", unique_filename);
            let file_type = determine_file_type(&extension);

            let response = MediaUploadResponse {
                file_url,
                file_type,
                file_size,
                file_hash,
            };

            tracing::info!("📎 Media upload completed for user {}: {}", claims.username, unique_filename);
            HttpResponse::Ok().json(
                ApiResponse::success("Media uploaded successfully", response)
            )
        }
        Err(e) => {
            tracing::error!("Failed to save uploaded file: {}", e);
            HttpResponse::InternalServerError().json(
                ApiResponse::<()>::error("Failed to save uploaded file")
            )
        }
    }
}

fn validate_file(temp_file: &TempFile) -> Result<(), HttpResponse> {
    // Check file size
    let file_size = match fs::metadata(&temp_file.file.path()) {
        Ok(metadata) => metadata.len(),
        Err(_) => {
            return Err(HttpResponse::BadRequest().json(
                ApiResponse::<()>::error("Unable to read file metadata")
            ));
        }
    };

    if file_size > MAX_FILE_SIZE {
        return Err(HttpResponse::BadRequest().json(
            ApiResponse::<()>::error(&format!("File size exceeds maximum limit of {}MB", MAX_FILE_SIZE / 1024 / 1024))
        ));
    }

    if file_size == 0 {
        return Err(HttpResponse::BadRequest().json(
            ApiResponse::<()>::error("File is empty")
        ));
    }

    // Check file extension from filename, or fall back to content type
    let mut is_valid = false;
    
    if let Some(filename) = &temp_file.file_name {
        let extension = get_file_extension(filename);
        if ALLOWED_EXTENSIONS.contains(&extension.as_str()) {
            is_valid = true;
        } else {
            tracing::warn!("File extension '{}' not in allowed list, checking content-type", extension);
        }
    }
    
    // If filename validation failed or no filename, check content type
    if !is_valid {
        if let Some(content_type) = &temp_file.content_type {
            let mime_str = content_type.to_string();
            tracing::info!("Checking content-type: {}", mime_str);
            
            // Map common mime types to extensions
            let allowed_mimes = vec![
                "image/jpeg", "image/jpg", "image/png", "image/gif",
                "video/mp4", "video/quicktime", "video/avi", "video/x-msvideo"
            ];
            
            if allowed_mimes.contains(&mime_str.as_str()) {
                is_valid = true;
                tracing::info!("Valid content-type found: {}", mime_str);
            }
        }
    }
    
    if !is_valid {
        return Err(HttpResponse::BadRequest().json(
            ApiResponse::<()>::error(&format!("File type not allowed. Supported types: {}", ALLOWED_EXTENSIONS.join(", ")))
        ));
    }

    Ok(())
}

fn get_file_extension(filename: &str) -> String {
    filename
        .split('.')
        .last()
        .unwrap_or("")
        .to_lowercase()
}

fn determine_file_type(extension: &str) -> String {
    match extension {
        "jpg" | "jpeg" | "png" | "gif" => "image".to_string(),
        "mp4" | "mov" | "avi" => "video".to_string(),
        _ => "unknown".to_string(),
    }
}

async fn calculate_file_hash(file_path: &std::path::Path) -> Result<String, std::io::Error> {
    let mut file = std::fs::File::open(file_path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

// Handler to serve uploaded media files
#[tracing::instrument(
    name = "Serve workout media",
    skip(path),
)]
pub async fn serve_workout_media(
    path: web::Path<String>,
) -> HttpResponse {
    let filename = path.into_inner();
    tracing::info!("🖼️ [SERVE MEDIA DEBUG] Requested filename: {}", filename);
    
    // Validate filename to prevent directory traversal
    if filename.contains("..") || filename.contains("/") || filename.contains("\\") {
        tracing::warn!("🚨 Directory traversal attempt detected: {}", filename);
        return HttpResponse::BadRequest().json(
            ApiResponse::<()>::error("Invalid filename")
        );
    }

    let file_path = PathBuf::from(UPLOAD_DIR).join(&filename);
    tracing::info!("🖼️ [SERVE MEDIA DEBUG] Looking for file at: {}", file_path.display());
    tracing::info!("🖼️ [SERVE MEDIA DEBUG] UPLOAD_DIR constant: {}", UPLOAD_DIR);
    
    match actix_web::web::block(move || std::fs::read(file_path)).await {
        Ok(Ok(contents)) => {
            let content_type = match get_file_extension(&filename).as_str() {
                "jpg" | "jpeg" => "image/jpeg",
                "png" => "image/png",
                "gif" => "image/gif",
                "mp4" => "video/mp4",
                "mov" => "video/quicktime",
                "avi" => "video/avi",
                _ => "application/octet-stream",
            };

            HttpResponse::Ok()
                .content_type(content_type)
                .body(contents)
        }
        Ok(Err(_)) | Err(_) => {
            HttpResponse::NotFound().json(
                ApiResponse::<()>::error("File not found")
            )
        }
    }
}