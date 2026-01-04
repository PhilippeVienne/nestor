output "asg_id" {
  description = "ID of the Auto Scaling Group"
  value       = aws_autoscaling_group.whisper.id
}

output "asg_arn" {
  description = "ARN of the Auto Scaling Group"
  value       = aws_autoscaling_group.whisper.arn
}

output "asg_name" {
  description = "Name of the Auto Scaling Group"
  value       = aws_autoscaling_group.whisper.name
}

output "security_group_id" {
  description = "ID of the security group"
  value       = aws_security_group.whisper.id
}

output "iam_role_arn" {
  description = "ARN of the IAM role"
  value       = aws_iam_role.whisper.arn
}

output "iam_role_name" {
  description = "Name of the IAM role"
  value       = aws_iam_role.whisper.name
}

output "launch_template_id" {
  description = "ID of the launch template"
  value       = aws_launch_template.whisper.id
}
