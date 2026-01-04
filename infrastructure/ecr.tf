resource "aws_ecr_repository" "app" {
  name                 = local.ecr_repository_name
  image_tag_mutability = var.ecr_image_tag_mutability
  region = var.aws_region

  image_scanning_configuration {
    scan_on_push = var.ecr_scan_on_push
  }

  encryption_configuration {
    encryption_type = var.enable_kms_encryption ? "KMS" : "AES256"
    kms_key         = var.enable_kms_encryption ? (var.kms_key_id != null ? var.kms_key_id : aws_kms_key.ecr[0].arn) : null
  }

  tags = {
    Name = local.ecr_repository_name
  }
}

resource "aws_ecr_lifecycle_policy" "app" {
  repository = aws_ecr_repository.app.name
  policy     = jsonencode(local.ecr_lifecycle_policy)
}

output "ecr_repository_url" {
  description = "URL of the ECR repository"
  value       = aws_ecr_repository.app.repository_url
}

output "ecr_repository_arn" {
  description = "ARN of the ECR repository"
  value       = aws_ecr_repository.app.arn
}

output "ecr_repository_name" {
  description = "Name of the ECR repository"
  value       = aws_ecr_repository.app.name
}