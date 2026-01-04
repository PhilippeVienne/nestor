# Nestor Audio Server - AI Coding Agent Instructions

## Project Overview
Nestor is an audio processing service that transcribes audio files and provides conversational AI based on the transcribed history. It's a serverless AWS architecture with three main components: FastAPI Lambda for API gateway, EC2-based speech recognition (OpenAI Whisper), and Bedrock for NLP.

**Architecture Pattern**: Async queue-based processing
- API Lambda receives requests → SQS → EC2 ASG for Whisper transcription
- Transcriptions stored in S3 → SQS → Lambda for Bedrock NLP processing
- PostgreSQL RDS stores conversation history and metadata

**Infrastructure Organization**: Modular Terraform structure in `infrastructure/modules/`:
- `sqs/` - Queue management with DLQ and CloudWatch alarms
- `dynamodb/` - Task results table with auto-scaling support
- `rds/` - PostgreSQL with automated backups and Secrets Manager integration
- `whisper-asg/` - EC2 Auto Scaling Group for Whisper processing
- `lambda-bedrock/` - Lambda function for Bedrock NLP
- `github-oidc/` - GitHub Actions OIDC provider and IAM roles

## Infrastructure (Terraform)

### Deployment Workflow
```bash
cd infrastructure/
./tf-init.sh              # Initialize backend using AWS Secrets Manager
terraform plan
terraform apply
```

**Critical**: The `tf-init.sh` script manages S3 backend configuration automatically. It creates/reads a secret (`terraform/backend/config`) in AWS Secrets Manager containing the state bucket name. Never manually configure backend - always use this script.

### Module Usage Pattern
All infrastructure is defined in [resources.tf](infrastructure/resources.tf) using modules:
```terraform
module "audio_processing_queue" {
  source = "./modules/sqs"
  queue_name = "${local.name_prefix}-audio-processing"
  # Module handles DLQ, alarms, encryption automatically
}
```

### Key Terraform Patterns
- **Naming convention**: Resources use `${var.project_name}-${var.environment}-<resource-type>` via `local.name_prefix` in [locals.tf](infrastructure/locals.tf)
- **S3 buckets**: Use randomized suffixes (e.g., `nestor-production-audio-recordings-<random>`) defined in `locals.tf`
- **IPv6 support**: VPC is IPv6-enabled by default (`vpc_enable_ipv6=true`), NAT gateway disabled for cost optimization
- **AMI Management**: Uses tag-based alias (`nestor-whisper-latest`) for zero-downtime updates
- **Security Groups**: RDS allows Lambda access via [resources.tf](infrastructure/resources.tf#L170)

### Resource Organization
- [ecr.tf](infrastructure/ecr.tf): Container registry for Lambda images
- [s3.tf](infrastructure/s3.tf): `audio-recordings` and `transcripts` buckets with encryption/versioning
- [iam.tf](infrastructure/iam.tf): Policies for ECR push/pull, S3 read/write, KMS access
- [kms.tf](infrastructure/kms.tf): Optional KMS encryption for ECR (enable via `enable_kms_encryption` var)
- [vpc.tf](infrastructure/vpc.tf): Uses `terraform-aws-modules/vpc/aws` module v6.0+
- [ssm.tf](infrastructure/ssm.tf): Parameter Store for runtime configuration
- [resources.tf](infrastructure/resources.tf): All module instantiations and wiring
- [data.tf](infrastructure/data.tf): AMI lookups using alias tags

### Database Options
The RDS module supports both Aurora Serverless v2 and traditional RDS PostgreSQL:
- **Aurora Serverless v2** (default): Scales to 0.5 ACU for cost optimization, cold start ~30s
- **RDS PostgreSQL**: Always-on instance, no cold start but higher baseline cost
- Toggle via `use_aurora_serverless` variable in [variables.tf](infrastructure/variables.tf)

## API (FastAPI Lambda)

### Technology Stack
- FastAPI + Mangum (Lambda adapter) - see [api/main.py](api/main.py)
- Pydantic Settings for configuration via env vars
- Boto3 for AWS SDK (SQS, DynamoDB)

### Key Patterns
```python
# Lambda handler entry point (main.py)
handler = Mangum(app, lifespan="off")  # Mangum wraps FastAPI for Lambda

# Task submission pattern
@app.post("/tasks", response_model=TaskResponse, status_code=202)
async def create_task(task: TaskRequest):
    message_id = await send_to_sqs(task)  # Returns immediately (async)
```

### Configuration
Environment variables loaded via [config.py](api/config.py) using `pydantic-settings`:
- `AWS_REGION`, `SQS_QUEUE_URL`, `DYNAMODB_TABLE`
- `ENVIRONMENT` (dev/production)
- Uses `.env.example` as template - create `.env` for local testing

### Container Build
```bash
cd api/
docker build -t nestor-api .
# Push to ECR (get URL from terraform output: ecr_repository_url)
aws ecr get-login-password | docker login --username AWS --password-stdin <ecr_url>
docker tag nestor-api:latest <ecr_url>:latest
docker push <ecr_url>:latest
```

### SQS Processor Pattern
[sqs_processor.py](api/sqs_processor.py) demonstrates the task processing framework:
- `process_task()` routes by `task_type` (speech_synthesis, audio_processing, data_transform)
- Results saved to DynamoDB with `message_id` as key
- **Extend by adding new task types to the routing logic**

## Speech AMI (Packer + Docker)

### Architecture
Whisper processing runs in Docker containers on EC2 instances:
- **Dockerfile**: [speech-ami/Dockerfile](speech-ami/Dockerfile) - Python 3.11 with Whisper and ffmpeg
- **Processor**: [speech-ami/process_audio.py](speech-ami/process_audio.py) - SQS polling loop with Whisper transcription
- **AMI**: Packer builds AMI with Docker pre-installed and image pre-built
- **User Data**: [modules/whisper-asg/user_data.sh](infrastructure/modules/whisper-asg/user_data.sh) simply starts the Docker container

### Building the AMI
```bash
cd speech-ami/
packer build nestor-audio-server.pkr.hcl
```

**What Packer Does**:
1. Installs Docker and AWS CLI
2. Copies Dockerfile and process_audio.py to AMI
3. Builds Docker image `nestor-whisper-processor:latest` inside AMI
4. Creates manifest.json with AMI ID

**Configuration**: See [nestor-audio-server.pkr.hcl](speech-ami/nestor-audio-server.pkr.hcl)
- Base: Ubuntu 22.04 LTS (Jammy)
- Variables: `aws_region` (default: eu-west-3), `instance_type` (default: t3.medium)

### AMI Alias System
GitHub Actions CI automatically manages AMI versioning:
1. Packer builds new AMI with timestamp in name
2. CI tags new AMI with `AmiAlias=nestor-whisper-latest`
3. Terraform data source finds AMI by alias tag (see [data.tf](infrastructure/data.tf))
4. ASG automatically uses latest AMI on next instance launch

**Manual AMI Updates**:
```bash
# Trigger AMI rebuild via GitHub Actions
gh workflow run build-ami.yml

# Or set specific AMI in terraform.tfvars
whisper_ami_id = "ami-0123456789abcdef0"
```

## CI/CD (GitHub Actions)

### Workflows
- [terraform.yml](.github/workflows/terraform.yml): Plan on PR, apply on main branch push
- [api-deploy.yml](.github/workflows/api-deploy.yml): Build/push Docker image, update Lambda
- [build-ami.yml](.github/workflows/build-ami.yml): Build AMI and update alias

### GitHub OIDC Authentication
Uses IAM role for secure, keyless authentication:
```yaml
- uses: aws-actions/configure-aws-credentials@v4
  with:
    role-to-assume: ${{ secrets.AWS_GITHUB_ACTIONS_ROLE_ARN }}
    aws-region: us-east-1
```

**Setup**: Terraform creates OIDC provider and role (see `modules/github-oidc/`)
1. Deploy infrastructure once with placeholder role
2. Add `AWS_GITHUB_ACTIONS_ROLE_ARN` to GitHub Secrets
3. Workflows authenticate automatically

**Role Permissions** (configured in [resources.tf](infrastructure/resources.tf#L150)):
- ECR: Push/pull container images
- S3: Access state bucket and application buckets
- Lambda: Update function code
- Full Terraform permissions for infra changes

## Project Conventions

### Resource Naming
All AWS resources follow the pattern: `{project_name}-{environment}-{resource_type}-{random_suffix?}`
Example: `nestor-production-audio-recordings-a1b2c3d4`

### State Management
- Terraform state stored in S3 bucket defined in AWS Secrets Manager
- Secret name: `terraform/backend/config` (override via `TERRAFORM_BACKEND_SECRET_NAME`)
- State key: `<folder_name>.tfstate` (override via `TERRAFORM_STATE_NAME`)

### Security Defaults
- S3 buckets: Public access blocked, encryption enabled (AES256 or KMS)
- ECR: Image scanning on push enabled by default
- VPC: Flow logs enabled with 7-day retention
- RDS: Passwords in Secrets Manager, automatic backups enabled
- Lambda: VPC-isolated with security group restrictions

### Common Gotchas
1. **Frontend folder is empty** - React app not yet implemented
2. **No NAT Gateway** - VPC uses IPv6 for outbound (cost optimization), ensure services support IPv6
3. **Lambda requires ZIP** - Initial deploy needs placeholder ZIP (see [infrastructure/lambda/bedrock-processor/](infrastructure/lambda/bedrock-processor/))
4. **Terraform init fails** - Ensure AWS credentials are configured and have Secrets Manager/S3 access
5. **AMI not found** - Run `build-ami.yml` workflow first, or manually build with Packer
6. **Whisper AMI changes** - Update AMI, then refresh ASG instances or update launch template

## Development Workflow

### Local Testing
```bash
# API local testing (requires AWS credentials)
cd api/
python -m venv venv && source venv/bin/activate
pip install -r requirements.txt
uvicorn main:app --reload

# Test Whisper processor locally
cd speech-ami/
docker build -t nestor-whisper-processor:latest .
docker run --rm \
  -e AWS_REGION=us-east-1 \
  -e SQS_QUEUE_URL=<queue-url> \
  -e AUDIO_BUCKET_NAME=<bucket> \
  -e TRANSCRIPT_BUCKET_NAME=<bucket> \
  nestor-whisper-processor:latest
```

### Adding New Infrastructure
1. Create module in `infrastructure/modules/<module-name>/`
2. Add module instantiation in [resources.tf](infrastructure/resources.tf)
3. Update [variables.tf](infrastructure/variables.tf) for configurable values
4. Follow existing patterns in [locals.tf](infrastructure/locals.tf) for naming

### Adding New API Endpoints
1. Define Pydantic models in [main.py](api/main.py)
2. Add route handlers with proper response models
3. Use `@app.<method>` decorators with explicit response models
4. For async work, submit to SQS via `send_to_sqs()`

### Testing Terraform Changes
```bash
terraform plan -out=plan.tfplan  # Review changes
terraform apply plan.tfplan      # Apply if safe
```

### Updating Whisper Processor
1. Modify [speech-ami/process_audio.py](speech-ami/process_audio.py) or [Dockerfile](speech-ami/Dockerfile)
2. Push to main branch (triggers AMI build automatically)
3. Wait for workflow to complete and tag new AMI
4. Refresh ASG instances: `aws autoscaling start-instance-refresh --auto-scaling-group-name <asg-name>`

## External Dependencies
- AWS Services: Lambda, API Gateway, SQS, S3, RDS (PostgreSQL), EC2 ASG, Bedrock, Secrets Manager, SSM Parameter Store
- OpenAI Whisper: Speech recognition (runs in Docker on EC2)
- Terraform AWS VPC Module: v6.0+ from `terraform-aws-modules/vpc/aws`
- GitHub Actions: CI/CD with OIDC authentication
