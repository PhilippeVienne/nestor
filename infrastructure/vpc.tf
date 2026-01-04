module "vpc" {
  source  = "terraform-aws-modules/vpc/aws"
  version = "~> 6.0"

  name = "${local.name_prefix}-vpc"
  cidr = var.vpc_cidr

  azs             = var.vpc_azs
  private_subnets = var.vpc_private_subnets
  public_subnets  = var.vpc_public_subnets

  # IPv6 Configuration
  enable_ipv6                                   = var.vpc_enable_ipv6
  public_subnet_assign_ipv6_address_on_creation = var.vpc_enable_ipv6

  # IPv6 CIDR blocks - Let AWS assign them
  public_subnet_ipv6_prefixes  = [0, 1, 2]
  private_subnet_ipv6_prefixes = [3, 4, 5]

  # NAT Gateway - Disabled for IPv6-only
  enable_nat_gateway = var.vpc_enable_nat_gateway
  single_nat_gateway = false

  # DNS
  enable_dns_hostnames = var.vpc_enable_dns_hostnames
  enable_dns_support   = var.vpc_enable_dns_support

  # VPC Flow Logs (optional but recommended)
  enable_flow_log                      = true
  create_flow_log_cloudwatch_iam_role  = true
  create_flow_log_cloudwatch_log_group = true
  flow_log_cloudwatch_log_group_retention_in_days = 7

  tags = {
    Name = "${local.name_prefix}-vpc"
  }

  public_subnet_tags = {
    Type = "public"
  }

  private_subnet_tags = {
    Type = "private"
  }
}

# Outputs
output "vpc_id" {
  description = "ID of the VPC"
  value       = module.vpc.vpc_id
}

output "vpc_cidr_block" {
  description = "CIDR block of the VPC"
  value       = module.vpc.vpc_cidr_block
}

output "vpc_ipv6_cidr_block" {
  description = "IPv6 CIDR block of the VPC"
  value       = module.vpc.vpc_ipv6_cidr_block
}

output "private_subnets" {
  description = "List of IDs of private subnets"
  value       = module.vpc.private_subnets
}

output "public_subnets" {
  description = "List of IDs of public subnets"
  value       = module.vpc.public_subnets
}

output "private_subnet_ipv6_cidr_blocks" {
  description = "List of IPv6 CIDR blocks of private subnets"
  value       = module.vpc.private_subnets_ipv6_cidr_blocks
}

output "public_subnet_ipv6_cidr_blocks" {
  description = "List of IPv6 CIDR blocks of public subnets"
  value       = module.vpc.public_subnets_ipv6_cidr_blocks
}

output "vpc_default_security_group_id" {
  description = "ID of the default security group"
  value       = module.vpc.default_security_group_id
}
