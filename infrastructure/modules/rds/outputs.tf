output "db_instance_id" {
  description = "ID of the database instance"
  value       = var.use_aurora_serverless ? aws_rds_cluster.aurora[0].id : aws_db_instance.main[0].id
}

output "db_instance_arn" {
  description = "ARN of the database instance"
  value       = var.use_aurora_serverless ? aws_rds_cluster.aurora[0].arn : aws_db_instance.main[0].arn
}

output "db_instance_endpoint" {
  description = "Connection endpoint"
  value       = var.use_aurora_serverless ? aws_rds_cluster.aurora[0].endpoint : aws_db_instance.main[0].endpoint
}

output "db_instance_address" {
  description = "Address of the database instance"
  value       = var.use_aurora_serverless ? aws_rds_cluster.aurora[0].endpoint : aws_db_instance.main[0].address
}

output "db_instance_port" {
  description = "Port of the database instance"
  value       = var.use_aurora_serverless ? aws_rds_cluster.aurora[0].port : aws_db_instance.main[0].port
}

output "db_instance_name" {
  description = "Database name"
  value       = var.database_name
}

output "db_master_username" {
  description = "Master username"
  value       = var.master_username
  sensitive   = true
}

output "db_cluster_id" {
  description = "ID of the Aurora cluster (null if not using Aurora)"
  value       = var.use_aurora_serverless ? aws_rds_cluster.aurora[0].id : null
}

output "db_reader_endpoint" {
  description = "Reader endpoint for Aurora cluster (null if not using Aurora)"
  value       = var.use_aurora_serverless ? aws_rds_cluster.aurora[0].reader_endpoint : null
}

output "db_security_group_id" {
  description = "Security group ID for RDS"
  value       = aws_security_group.rds.id
}

output "db_password_secret_arn" {
  description = "ARN of the Secrets Manager secret containing DB credentials"
  value       = aws_secretsmanager_secret.db_password.arn
}

output "db_password_secret_name" {
  description = "Name of the Secrets Manager secret containing DB credentials"
  value       = aws_secretsmanager_secret.db_password.name
}
