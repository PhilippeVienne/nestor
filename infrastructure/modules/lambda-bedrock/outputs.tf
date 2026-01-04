output "function_arn" {
  description = "ARN of the Lambda function"
  value       = aws_lambda_function.bedrock_processor.arn
}

output "function_name" {
  description = "Name of the Lambda function"
  value       = aws_lambda_function.bedrock_processor.function_name
}

output "function_invoke_arn" {
  description = "Invoke ARN of the Lambda function"
  value       = aws_lambda_function.bedrock_processor.invoke_arn
}

output "iam_role_arn" {
  description = "ARN of the Lambda IAM role"
  value       = aws_iam_role.lambda.arn
}

output "iam_role_name" {
  description = "Name of the Lambda IAM role"
  value       = aws_iam_role.lambda.name
}

output "security_group_id" {
  description = "ID of the Lambda security group"
  value       = aws_security_group.lambda.id
}

output "log_group_name" {
  description = "Name of the CloudWatch log group"
  value       = aws_cloudwatch_log_group.lambda.name
}
