#!/bin/bash

# Nestor Audio Server AMI Build Script
# Builds the Whisper processor AMI using Packer

set -e

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Default values
AWS_REGION="${AWS_REGION:-eu-west-3}"
INSTANCE_TYPE="${INSTANCE_TYPE:-t3.medium}"
UPDATE_ALIAS="${UPDATE_ALIAS:-false}"
AMI_ALIAS="nestor-whisper-latest"
VPC_ID="${VPC_ID:-}"
SUBNET_ID="${SUBNET_ID:-}"
AUTO_DETECT="${AUTO_DETECT:-true}"

# Function to print colored output
print_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

# Function to display usage
usage() {
    cat << EOF
Usage: $0 [OPTIONS]

Build the Nestor Whisper processor AMI using Packer.

Options:
    -r, --region REGION          AWS region (default: eu-west-3)
    -i, --instance-type TYPE     EC2 instance type for building (default: t3.medium)
    -u, --update-alias           Update AMI alias tag after build
    -p, --iam-profile PROFILE    IAM instance profile for builder instance
    -v, --vpc-id VPC_ID          VPC ID for builder instance
    -s, --subnet-id SUBNET_ID    Subnet ID for builder instance
    --no-auto-detect             Disable automatic VPC/subnet detection
    -h, --help                   Display this help message

Environment Variables:
    AWS_REGION                   AWS region (overridden by --region)
    INSTANCE_TYPE                EC2 instance type (overridden by --instance-type)
    UPDATE_ALIAS                 Set to 'true' to update alias (overridden by --update-alias)
    VPC_ID                       VPC ID (overridden by --vpc-id)
    SUBNET_ID                    Subnet ID (overridden by --subnet-id)

Examples:
    # Basic build (auto-detects VPC/subnet from Terraform state)
    ./build-ami.sh

    # Build and update alias
    ./build-ami.sh --update-alias

    # Build with explicit VPC/subnet
    ./build-ami.sh --vpc-id vpc-xxxxx --subnet-id subnet-xxxxx

    # Build in specific region with custom instance type
    ./build-ami.sh --region us-east-1 --instance-type t3.large

EOF
    exit 1
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -r|--region)
            AWS_REGION="$2"
            shift 2
            ;;
        -i|--instance-type)
            INSTANCE_TYPE="$2"
            shift 2
            ;;
        -u|--update-alias)
            UPDATE_ALIAS="true"
            shift
            ;;
        -p|--iam-profile)
            IAM_PROFILE="$2"
            shift 2
            ;;
        -v|--vpc-id)
            VPC_ID="$2"
            shift 2
            ;;
        -s|--subnet-id)
            SUBNET_ID="$2"
            shift 2
            ;;
        --no-auto-detect)
            AUTO_DETECT="false"
            shift
            ;;
        -h|--help)
            usage
            ;;
        *)
            print_error "Unknown option: $1"
            usage
            ;;
    esac
done

# Check prerequisites
print_info "Checking prerequisites..."

if ! command -v packer &> /dev/null; then
    print_error "Packer is not installed. Please install Packer first."
    print_info "Visit: https://www.packer.io/downloads"
    exit 1
fi

if ! command -v aws &> /dev/null; then
    print_error "AWS CLI is not installed. Please install AWS CLI first."
    exit 1
fi

# Check AWS credentials
print_info "Verifying AWS credentials..."
if ! aws sts get-caller-identity &> /dev/null; then
    print_error "AWS credentials not configured or invalid."
    print_info "Run 'aws configure' or set AWS environment variables."
    exit 1
fi

# Auto-detect VPC and subnet if not provided
if [ "$AUTO_DETECT" = "true" ] && [ -z "$VPC_ID" ] && [ -z "$SUBNET_ID" ]; then
    print_info "Auto-detecting VPC and subnet from Terraform state..."
    
    # Try to get VPC ID and subnet from terraform state
    if [ -f "../infrastructure/terraform.tfstate" ]; then
        print_info "Reading from Terraform state..."
        VPC_ID=$(cd ../infrastructure && terraform output -raw vpc_id 2>/dev/null) || VPC_ID=""
        if [ -n "$VPC_ID" ]; then
            # Get the first public subnet
            SUBNET_ID=$(cd ../infrastructure && terraform output -json public_subnet_ids 2>/dev/null | jq -r '.[0]' 2>/dev/null) || SUBNET_ID=""
        fi
    fi
    
    # If still empty, try to find the default VPC
    if [ -z "$VPC_ID" ]; then
        print_warning "Could not read from Terraform state, attempting to find default or any VPC..."
        VPC_ID=$(aws ec2 describe-vpcs \
            --region "$AWS_REGION" \
            --filters "Name=is-default,Values=true" \
            --query 'Vpcs[0].VpcId' \
            --output text 2>/dev/null)
        
        if [ "$VPC_ID" = "None" ] || [ -z "$VPC_ID" ]; then
            # No default VPC, try to find any VPC
            VPC_ID=$(aws ec2 describe-vpcs \
                --region "$AWS_REGION" \
                --query 'Vpcs[0].VpcId' \
                --output text 2>/dev/null)
        fi
    fi
    
    # If we have VPC but no subnet, find a public subnet
    if [ -n "$VPC_ID" ] && [ "$VPC_ID" != "None" ] && [ -z "$SUBNET_ID" ]; then
        print_info "Finding a public subnet in VPC $VPC_ID..."
        SUBNET_ID=$(aws ec2 describe-subnets \
            --region "$AWS_REGION" \
            --filters "Name=vpc-id,Values=$VPC_ID" "Name=map-public-ip-on-launch,Values=true" \
            --query 'Subnets[0].SubnetId' \
            --output text 2>/dev/null)
        
        if [ "$SUBNET_ID" = "None" ] || [ -z "$SUBNET_ID" ]; then
            # No public subnet found, try any subnet
            SUBNET_ID=$(aws ec2 describe-subnets \
                --region "$AWS_REGION" \
                --filters "Name=vpc-id,Values=$VPC_ID" \
                --query 'Subnets[0].SubnetId' \
                --output text 2>/dev/null)
        fi
    fi
    
    if [ -z "$VPC_ID" ] || [ "$VPC_ID" = "None" ]; then
        print_error "Could not auto-detect VPC. Please specify VPC ID and Subnet ID:"
        print_info "  ./build-ami.sh --vpc-id vpc-xxxxx --subnet-id subnet-xxxxx"
        print_info "Or disable auto-detection and let Packer fail if no default VPC exists:"
        print_info "  ./build-ami.sh --no-auto-detect"
        exit 1
    fi
    
    if [ -z "$SUBNET_ID" ] || [ "$SUBNET_ID" = "None" ]; then
        print_error "Could not auto-detect subnet. Please specify Subnet ID:"
        print_info "  ./build-ami.sh --vpc-id $VPC_ID --subnet-id subnet-xxxxx"
        exit 1
    fi
    
    print_info "Auto-detected VPC: $VPC_ID"
    print_info "Auto-detected Subnet: $SUBNET_ID"
fi

# Display build configuration
print_info "Build Configuration:"
echo "  Region:          $AWS_REGION"
echo "  Instance Type:   $INSTANCE_TYPE"
echo "  Update Alias:    $UPDATE_ALIAS"
if [ -n "$IAM_PROFILE" ]; then
    echo "  IAM Profile:     $IAM_PROFILE"
fi
if [ -n "$VPC_ID" ]; then
    echo "  VPC ID:          $VPC_ID"
fi
if [ -n "$SUBNET_ID" ]; then
    echo "  Subnet ID:       $SUBNET_ID"
fi

# Initialize Packer
print_info "Initializing Packer..."
packer init nestor-audio-server.pkr.hcl

# Build AMI
print_info "Building AMI with Packer..."
print_warning "This may take 10-15 minutes..."

PACKER_ARGS="-var aws_region=$AWS_REGION -var instance_type=$INSTANCE_TYPE"
if [ -n "$IAM_PROFILE" ]; then
    PACKER_ARGS="$PACKER_ARGS -var iam_instance_profile=$IAM_PROFILE"
fi
if [ -n "$VPC_ID" ]; then
    PACKER_ARGS="$PACKER_ARGS -var vpc_id=$VPC_ID"
fi
if [ -n "$SUBNET_ID" ]; then
    PACKER_ARGS="$PACKER_ARGS -var subnet_id=$SUBNET_ID"
fi

if packer build $PACKER_ARGS nestor-audio-server.pkr.hcl; then
    print_info "AMI build completed successfully!"
else
    print_error "AMI build failed!"
    exit 1
fi

# Extract AMI ID from manifest
if [ -f "manifest.json" ]; then
    AMI_ID=$(jq -r '.builds[0].artifact_id' manifest.json | cut -d':' -f2)
    print_info "AMI ID: $AMI_ID"
    
    # Update alias tag if requested
    if [ "$UPDATE_ALIAS" = "true" ]; then
        print_info "Updating AMI alias tag..."
        
        # Remove alias from any existing AMIs
        print_info "Removing alias from previous AMIs..."
        OLD_AMIS=$(aws ec2 describe-images \
            --region "$AWS_REGION" \
            --owners self \
            --filters "Name=tag:AmiAlias,Values=$AMI_ALIAS" \
            --query 'Images[*].ImageId' \
            --output text)
        
        if [ -n "$OLD_AMIS" ]; then
            for OLD_AMI in $OLD_AMIS; do
                print_info "Removing alias tag from $OLD_AMI"
                aws ec2 delete-tags \
                    --region "$AWS_REGION" \
                    --resources "$OLD_AMI" \
                    --tags Key=AmiAlias || true
            done
        fi
        
        # Add alias tag to new AMI
        print_info "Adding alias tag to new AMI $AMI_ID"
        aws ec2 create-tags \
            --region "$AWS_REGION" \
            --resources "$AMI_ID" \
            --tags Key=AmiAlias,Value=$AMI_ALIAS
        
        print_info "AMI alias '$AMI_ALIAS' updated successfully!"
    fi
    
    echo ""
    print_info "================================"
    print_info "AMI Build Summary"
    print_info "================================"
    echo "  AMI ID:      $AMI_ID"
    echo "  Region:      $AWS_REGION"
    if [ "$UPDATE_ALIAS" = "true" ]; then
        echo "  Alias:       $AMI_ALIAS"
    fi
    echo ""
    print_info "To use this AMI in Terraform:"
    echo "  whisper_ami_id = \"$AMI_ID\""
    echo ""
    if [ "$UPDATE_ALIAS" = "false" ]; then
        print_info "To update the AMI alias, run:"
        echo "  ./build-ami.sh --update-alias"
    fi
else
    print_warning "manifest.json not found. Could not extract AMI ID."
fi

print_info "Done!"
