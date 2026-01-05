# ECR Repository Outputs
output "repository_url" {
  description = "URL of the ECR repository"
  value       = aws_ecr_repository.main.repository_url
}

output "repository_arn" {
  description = "ARN of the ECR repository"
  value       = aws_ecr_repository.main.arn
}

output "repository_name" {
  description = "Name of the ECR repository"
  value       = aws_ecr_repository.main.name
}

output "repository_registry_id" {
  description = "Registry ID where the repository was created"
  value       = aws_ecr_repository.main.registry_id
}

# KMS Outputs
output "kms_key_id" {
  description = "KMS key ID for ECR encryption"
  value       = var.enable_kms_encryption && var.kms_key_id == null ? aws_kms_key.ecr[0].id : var.kms_key_id
}

output "kms_key_arn" {
  description = "KMS key ARN for ECR encryption"
  value       = var.enable_kms_encryption && var.kms_key_id == null ? aws_kms_key.ecr[0].arn : var.kms_key_id
}

# IAM Policy Outputs
output "iam_policy_ecr_push_arn" {
  description = "ARN of the IAM policy for ECR push access"
  value       = var.create_iam_policies ? aws_iam_policy.ecr_push[0].arn : null
}

output "iam_policy_ecr_pull_arn" {
  description = "ARN of the IAM policy for ECR pull access"
  value       = var.create_iam_policies ? aws_iam_policy.ecr_pull[0].arn : null
}

output "iam_policy_ecr_admin_arn" {
  description = "ARN of the IAM policy for ECR admin access"
  value       = var.create_iam_policies ? aws_iam_policy.ecr_admin[0].arn : null
}

output "iam_policy_ecr_kms_arn" {
  description = "ARN of the IAM policy for ECR KMS operations"
  value       = var.enable_kms_encryption && var.create_iam_policies ? aws_iam_policy.ecr_kms[0].arn : null
}
