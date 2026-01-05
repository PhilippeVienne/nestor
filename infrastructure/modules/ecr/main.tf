# ECR Repository
resource "aws_ecr_repository" "main" {
  name                 = var.repository_name
  image_tag_mutability = var.image_tag_mutability

  image_scanning_configuration {
    scan_on_push = var.scan_on_push
  }

  encryption_configuration {
    encryption_type = var.enable_kms_encryption ? "KMS" : "AES256"
    kms_key         = var.enable_kms_encryption ? (var.kms_key_id != null ? var.kms_key_id : aws_kms_key.ecr[0].arn) : null
  }

  tags = var.tags
}

# ECR Lifecycle Policy
resource "aws_ecr_lifecycle_policy" "main" {
  repository = aws_ecr_repository.main.name
  policy     = jsonencode(var.lifecycle_policy)
}

# KMS Key for ECR encryption (optional)
resource "aws_kms_key" "ecr" {
  count = var.enable_kms_encryption && var.kms_key_id == null ? 1 : 0

  description             = "KMS key for ECR encryption"
  deletion_window_in_days = var.kms_deletion_window_days
  enable_key_rotation     = true

  tags = merge(var.tags, {
    Name = "${var.repository_name}-kms"
  })
}

resource "aws_kms_alias" "ecr" {
  count = var.enable_kms_encryption && var.kms_key_id == null ? 1 : 0

  name          = "alias/${var.repository_name}-ecr"
  target_key_id = aws_kms_key.ecr[0].key_id
}

# IAM Policy for ECR Push (CI/CD pipelines)
resource "aws_iam_policy" "ecr_push" {
  count = var.create_iam_policies ? 1 : 0

  name        = "${var.repository_name}-ecr-push"
  description = "Policy to allow pushing images to ECR"

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid    = "ECRGetAuthorizationToken"
        Effect = "Allow"
        Action = [
          "ecr:GetAuthorizationToken"
        ]
        Resource = "*"
      },
      {
        Sid    = "ECRPushImage"
        Effect = "Allow"
        Action = [
          "ecr:BatchCheckLayerAvailability",
          "ecr:CompleteLayerUpload",
          "ecr:InitiateLayerUpload",
          "ecr:PutImage",
          "ecr:UploadLayerPart"
        ]
        Resource = aws_ecr_repository.main.arn
      },
      {
        Sid    = "ECRGetDownloadUrl"
        Effect = "Allow"
        Action = [
          "ecr:GetDownloadUrlForLayer",
          "ecr:BatchGetImage"
        ]
        Resource = aws_ecr_repository.main.arn
      }
    ]
  })

  tags = var.tags
}

# IAM Policy for ECR Pull (Application servers, ECS tasks, etc.)
resource "aws_iam_policy" "ecr_pull" {
  count = var.create_iam_policies ? 1 : 0

  name        = "${var.repository_name}-ecr-pull"
  description = "Policy to allow pulling images from ECR"

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid    = "ECRGetAuthorizationToken"
        Effect = "Allow"
        Action = [
          "ecr:GetAuthorizationToken"
        ]
        Resource = "*"
      },
      {
        Sid    = "ECRPullImage"
        Effect = "Allow"
        Action = [
          "ecr:GetDownloadUrlForLayer",
          "ecr:BatchGetImage",
          "ecr:BatchCheckLayerAvailability"
        ]
        Resource = aws_ecr_repository.main.arn
      }
    ]
  })

  tags = var.tags
}

# IAM Policy for ECR Full Access (Admin operations)
resource "aws_iam_policy" "ecr_admin" {
  count = var.create_iam_policies ? 1 : 0

  name        = "${var.repository_name}-ecr-admin"
  description = "Policy to allow full ECR repository management"

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid    = "ECRGetAuthorizationToken"
        Effect = "Allow"
        Action = [
          "ecr:GetAuthorizationToken"
        ]
        Resource = "*"
      },
      {
        Sid    = "ECRFullAccess"
        Effect = "Allow"
        Action = [
          "ecr:*"
        ]
        Resource = aws_ecr_repository.main.arn
      }
    ]
  })

  tags = var.tags
}

# IAM Policy for KMS encryption (if enabled)
resource "aws_iam_policy" "ecr_kms" {
  count = var.enable_kms_encryption && var.create_iam_policies ? 1 : 0

  name        = "${var.repository_name}-ecr-kms"
  description = "Policy to allow KMS operations for ECR encryption"

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid    = "KMSDecryptForECR"
        Effect = "Allow"
        Action = [
          "kms:Decrypt",
          "kms:DescribeKey"
        ]
        Resource = var.kms_key_id != null ? var.kms_key_id : aws_kms_key.ecr[0].arn
      }
    ]
  })

  tags = var.tags
}
