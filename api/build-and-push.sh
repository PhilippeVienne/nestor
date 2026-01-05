#!/bin/bash
set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
PARAMETER_PREFIX="${PARAMETER_PREFIX:-/nestor-0-base}"
AWS_REGION="${AWS_REGION:-eu-west-3}"
IMAGE_TAG="${IMAGE_TAG:-latest}"

echo -e "${GREEN}=== Nestor API Docker Build & Push ===${NC}"
echo ""

# Check if AWS CLI is available
if ! command -v aws &> /dev/null; then
    echo -e "${RED}Error: AWS CLI is not installed${NC}"
    exit 1
fi

# Check AWS credentials
echo -e "${YELLOW}Checking AWS credentials...${NC}"
if ! aws sts get-caller-identity &> /dev/null; then
    echo -e "${RED}Error: AWS credentials are not configured${NC}"
    exit 1
fi
echo -e "${GREEN}✓ AWS credentials validated${NC}"
echo ""

# Fetch ECR repository URL from SSM Parameter Store
echo -e "${YELLOW}Fetching ECR repository URL from SSM Parameter Store...${NC}"
ECR_REPOSITORY_URL=$(aws ssm get-parameter \
    --name "${PARAMETER_PREFIX}/ecr_repository_url" \
    --with-decryption \
    --query 'Parameter.Value' \
    --output text \
    --region "${AWS_REGION}")

if [ -z "$ECR_REPOSITORY_URL" ]; then
    echo -e "${RED}Error: Could not retrieve ECR repository URL from SSM${NC}"
    echo -e "${RED}Parameter: ${PARAMETER_PREFIX}/ecr_repository_url${NC}"
    exit 1
fi

echo -e "${GREEN}✓ ECR Repository URL: ${ECR_REPOSITORY_URL}${NC}"
echo ""

# Extract AWS account ID and region from ECR URL
AWS_ACCOUNT_ID=$(echo "$ECR_REPOSITORY_URL" | cut -d'.' -f1 | cut -d'/' -f3)
ECR_REGION=$(echo "$ECR_REPOSITORY_URL" | cut -d'.' -f4)

# Login to ECR
echo -e "${YELLOW}Logging in to Amazon ECR...${NC}"
aws ecr get-login-password --region "${ECR_REGION}" | \
    docker login --username AWS --password-stdin "${AWS_ACCOUNT_ID}.dkr.ecr.${ECR_REGION}.amazonaws.com"

if [ $? -ne 0 ]; then
    echo -e "${RED}Error: Failed to login to ECR${NC}"
    exit 1
fi
echo -e "${GREEN}✓ Successfully logged in to ECR${NC}"
echo ""

# Build Docker image
echo -e "${YELLOW}Building Docker image...${NC}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

docker build -t nestor-api:${IMAGE_TAG} .

if [ $? -ne 0 ]; then
    echo -e "${RED}Error: Docker build failed${NC}"
    exit 1
fi
echo -e "${GREEN}✓ Docker image built successfully${NC}"
echo ""

# Tag the image
echo -e "${YELLOW}Tagging image...${NC}"
docker tag nestor-api:${IMAGE_TAG} ${ECR_REPOSITORY_URL}:${IMAGE_TAG}
echo -e "${GREEN}✓ Image tagged: ${ECR_REPOSITORY_URL}:${IMAGE_TAG}${NC}"
echo ""

# Push to ECR
echo -e "${YELLOW}Pushing image to ECR...${NC}"
docker push ${ECR_REPOSITORY_URL}:${IMAGE_TAG}

if [ $? -ne 0 ]; then
    echo -e "${RED}Error: Failed to push image to ECR${NC}"
    exit 1
fi
echo -e "${GREEN}✓ Image pushed successfully${NC}"
echo ""

# Display summary
echo -e "${GREEN}=== Build & Push Complete ===${NC}"
echo -e "Repository: ${ECR_REPOSITORY_URL}"
echo -e "Image Tag:  ${IMAGE_TAG}"
echo -e "Full Image: ${ECR_REPOSITORY_URL}:${IMAGE_TAG}"
echo ""
echo -e "${YELLOW}Next steps:${NC}"
echo -e "  - Update Lambda function to use this image"
echo -e "  - Or deploy via GitHub Actions workflow"
