#!/bin/bash

set -e

# This script initializes Terraform in the current directory using information setup in the current AWS profile.
# It ensures that the necessary backend configuration is in place for state management.

AWS_ACCOUNT_ID=$(aws sts get-caller-identity --query Account --output text)
AWS_REGION=$(aws configure get region)

SECRET_NAME=${TERRAFORM_BACKEND_SECRET_NAME:-"terraform/backend/config"}

setup_secret() {
  echo "Setting up Terraform backend secret in AWS Secrets Manager..."

  # Create the secret if it doesn't exist
  if ! aws secretsmanager describe-secret --secret-id "$SECRET_NAME" >/dev/null 2>&1; then
    aws secretsmanager create-secret --name "$SECRET_NAME" --description "Terraform backend configuration"
    echo "Created secret: $SECRET_NAME"
    # Create a bucket in the current region for Terraform state and store its name in the secret, randomize the suffix to avoid collisions
    BUCKET_NAME="terraform-state-$(openssl rand -hex 8)"
    aws s3api create-bucket --bucket "$BUCKET_NAME" --region "$AWS_REGION" --create-bucket-configuration LocationConstraint="$AWS_REGION"
    echo "Created S3 bucket: $BUCKET_NAME"
    aws secretsmanager put-secret-value --secret-id "$SECRET_NAME" --secret-string "{\"bucket_name\":\"$BUCKET_NAME\",\"region\":\"$AWS_REGION\"}"
    echo "Stored backend configuration in secret."
  else
    echo "Secret $SECRET_NAME already exists."
  fi
}

read_backend_config() {
  echo "Reading Terraform backend configuration from AWS Secrets Manager..."
  SECRET_VALUE=$(aws secretsmanager get-secret-value --secret-id "$SECRET_NAME" --query SecretString --output text)
  BUCKET_NAME=$(echo "$SECRET_VALUE" | jq -r '.bucket_name')
  REGION=$(echo "$SECRET_VALUE" | jq -r '.region')
  echo "Retrieved backend configuration: bucket_name=$BUCKET_NAME, region=$REGION"
}

initialize_terraform() {
  echo "Initializing Terraform..."
  CURRENT_FOLDER=$(pwd)
  CURRENT_FOLDER_NAME=$(basename "$CURRENT_FOLDER")
  STATE_NAME=${TERRAFORM_STATE_NAME:-"$CURRENT_FOLDER_NAME.tfstate"}
  terraform init -backend-config="bucket=$BUCKET_NAME" -backend-config="region=$REGION" -backend-config="key=$STATE_NAME" $@
}

setup_secret
read_backend_config
initialize_terraform $@
