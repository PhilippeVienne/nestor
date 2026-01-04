output "table_id" {
  description = "ID of the DynamoDB table"
  value       = aws_dynamodb_table.main.id
}

output "table_arn" {
  description = "ARN of the DynamoDB table"
  value       = aws_dynamodb_table.main.arn
}

output "table_name" {
  description = "Name of the DynamoDB table"
  value       = aws_dynamodb_table.main.name
}

output "stream_arn" {
  description = "ARN of the DynamoDB stream"
  value       = aws_dynamodb_table.main.stream_arn
}

output "stream_label" {
  description = "Label of the DynamoDB stream"
  value       = aws_dynamodb_table.main.stream_label
}
