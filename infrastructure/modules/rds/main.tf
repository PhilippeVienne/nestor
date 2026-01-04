# Security Group for RDS/Aurora
resource "aws_security_group" "rds" {
  name_prefix = "${var.identifier}-db-"
  description = "Security group for database instance"
  vpc_id      = var.vpc_id

  tags = merge(var.tags, {
    Name = "${var.identifier}-db-sg"
  })

  lifecycle {
    create_before_destroy = true
  }
}

resource "aws_vpc_security_group_ingress_rule" "rds_postgres" {
  for_each = toset(var.allowed_security_group_ids)

  security_group_id            = aws_security_group.rds.id
  referenced_security_group_id = each.value
  from_port                    = 5432
  to_port                      = 5432
  ip_protocol                  = "tcp"
  description                  = "PostgreSQL access from application"
}

resource "aws_vpc_security_group_egress_rule" "rds_all" {
  security_group_id = aws_security_group.rds.id
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "-1"
  description       = "Allow all outbound traffic"
}

# DB Subnet Group
resource "aws_db_subnet_group" "main" {
  name       = "${var.identifier}-subnet-group"
  subnet_ids = var.subnet_ids

  tags = merge(var.tags, {
    Name = "${var.identifier}-subnet-group"
  })
}

# Random password for master user
resource "random_password" "master" {
  length  = 32
  special = true
}

# Store password in Secrets Manager
resource "aws_secretsmanager_secret" "db_password" {
  name_prefix             = "${var.identifier}-db-password-"
  description             = "Master password for ${var.identifier} RDS instance"
  recovery_window_in_days = var.secret_recovery_window_days

  tags = var.tags
}

resource "aws_secretsmanager_secret_version" "db_password" {
  secret_id = aws_secretsmanager_secret.db_password.id
  secret_string = jsonencode({
    username = var.master_username
    password = random_password.master.result
    engine   = var.use_aurora_serverless ? "aurora-postgresql" : "postgres"
    host     = var.use_aurora_serverless ? aws_rds_cluster.aurora[0].endpoint : aws_db_instance.main[0].address
    port     = var.use_aurora_serverless ? aws_rds_cluster.aurora[0].port : aws_db_instance.main[0].port
    dbname   = var.database_name
  })
}

# Aurora Serverless v2 Cluster
resource "aws_rds_cluster" "aurora" {
  count = var.use_aurora_serverless ? 1 : 0

  cluster_identifier = var.identifier
  engine             = "aurora-postgresql"
  engine_mode        = "provisioned"
  engine_version     = var.aurora_engine_version
  database_name      = var.database_name
  master_username    = var.master_username
  master_password    = random_password.master.result
  port               = 5432

  # Network
  db_subnet_group_name   = aws_db_subnet_group.main.name
  vpc_security_group_ids = [aws_security_group.rds.id]

  # Serverless v2 scaling
  serverlessv2_scaling_configuration {
    min_capacity = var.aurora_min_capacity
    max_capacity = var.aurora_max_capacity
  }

  # Backup
  backup_retention_period      = var.backup_retention_period
  preferred_backup_window      = var.backup_window
  preferred_maintenance_window = var.maintenance_window
  skip_final_snapshot          = var.skip_final_snapshot
  final_snapshot_identifier    = var.skip_final_snapshot ? null : "${var.identifier}-final-snapshot-${formatdate("YYYY-MM-DD-hhmm", timestamp())}"

  # Encryption
  storage_encrypted = var.storage_encrypted
  kms_key_id        = var.kms_key_id

  # Monitoring
  enabled_cloudwatch_logs_exports = var.enabled_cloudwatch_logs_exports
  apply_immediately              = var.apply_immediately
  deletion_protection            = var.deletion_protection

  # Parameter groups
  db_cluster_parameter_group_name = var.cluster_parameter_group_name != null ? var.cluster_parameter_group_name : aws_rds_cluster_parameter_group.aurora[0].name

  tags = var.tags

  lifecycle {
    ignore_changes = [
      final_snapshot_identifier
    ]
  }
}

# Aurora Serverless v2 instance
resource "aws_rds_cluster_instance" "aurora" {
  count = var.use_aurora_serverless ? 1 : 0

  identifier         = "${var.identifier}-instance-1"
  cluster_identifier = aws_rds_cluster.aurora[0].id
  instance_class     = "db.serverless"
  engine             = aws_rds_cluster.aurora[0].engine
  engine_version     = aws_rds_cluster.aurora[0].engine_version

  # Monitoring
  monitoring_interval = var.monitoring_interval
  monitoring_role_arn = var.monitoring_interval > 0 ? aws_iam_role.rds_monitoring[0].arn : null
  
  performance_insights_enabled          = var.performance_insights_enabled
  performance_insights_retention_period = var.performance_insights_enabled ? var.performance_insights_retention_period : null

  # Parameter group
  db_parameter_group_name = var.parameter_group_name != null ? var.parameter_group_name : aws_db_parameter_group.main[0].name

  tags = var.tags
}

# RDS Instance (non-Aurora)
resource "aws_db_instance" "main" {
  count = var.use_aurora_serverless ? 0 : 1

  identifier = var.identifier

  # Engine
  engine               = "postgres"
  engine_version       = var.engine_version
  instance_class       = var.instance_class
  allocated_storage    = var.allocated_storage
  max_allocated_storage = var.max_allocated_storage
  storage_type         = var.storage_type
  storage_encrypted    = var.storage_encrypted
  kms_key_id           = var.kms_key_id

  # Database
  db_name  = var.database_name
  username = var.master_username
  password = random_password.master.result
  port     = 5432

  # Network
  db_subnet_group_name   = aws_db_subnet_group.main.name
  vpc_security_group_ids = [aws_security_group.rds.id]
  publicly_accessible    = var.publicly_accessible

  # Backup
  backup_retention_period   = var.backup_retention_period
  backup_window            = var.backup_window
  delete_automated_backups = var.delete_automated_backups
  skip_final_snapshot      = var.skip_final_snapshot
  final_snapshot_identifier = var.skip_final_snapshot ? null : "${var.identifier}-final-snapshot-${formatdate("YYYY-MM-DD-hhmm", timestamp())}"

  # Maintenance
  maintenance_window              = var.maintenance_window
  auto_minor_version_upgrade      = var.auto_minor_version_upgrade
  allow_major_version_upgrade     = var.allow_major_version_upgrade
  apply_immediately              = var.apply_immediately
  deletion_protection            = var.deletion_protection

  # Monitoring
  enabled_cloudwatch_logs_exports = var.enabled_cloudwatch_logs_exports
  monitoring_interval            = var.monitoring_interval
  monitoring_role_arn            = var.monitoring_interval > 0 ? aws_iam_role.rds_monitoring[0].arn : null
  performance_insights_enabled   = var.performance_insights_enabled
  performance_insights_retention_period = var.performance_insights_enabled ? var.performance_insights_retention_period : null

  # Parameter group
  parameter_group_name = var.parameter_group_name != null ? var.parameter_group_name : aws_db_parameter_group.main[0].name

  tags = var.tags

  lifecycle {
    ignore_changes = [
      final_snapshot_identifier
    ]
  }
}

# Aurora cluster parameter group
resource "aws_rds_cluster_parameter_group" "aurora" {
  count = var.use_aurora_serverless && var.cluster_parameter_group_name == null ? 1 : 0

  name_prefix = "${var.identifier}-cluster-"
  family      = var.aurora_parameter_group_family

  dynamic "parameter" {
    for_each = var.cluster_parameters
    content {
      name  = parameter.value.name
      value = parameter.value.value
    }
  }

  tags = var.tags

  lifecycle {
    create_before_destroy = true
  }
}

# DB parameter group (used by both Aurora instance and RDS)
resource "aws_db_parameter_group" "main" {
  count = var.parameter_group_name == null ? 1 : 0

  name_prefix = "${var.identifier}-"
  family      = var.use_aurora_serverless ? var.aurora_parameter_group_family : var.parameter_group_family

  dynamic "parameter" {
    for_each = var.parameters
    content {
      name  = parameter.value.name
      value = parameter.value.value
    }
  }

  tags = var.tags

  lifecycle {
    create_before_destroy = true
  }
}

# IAM role for enhanced monitoring
resource "aws_iam_role" "rds_monitoring" {
  count = var.monitoring_interval > 0 ? 1 : 0

  name_prefix = "${var.identifier}-rds-monitoring-"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Action = "sts:AssumeRole"
      Effect = "Allow"
      Principal = {
        Service = "monitoring.rds.amazonaws.com"
      }
    }]
  })

  tags = var.tags
}

resource "aws_iam_role_policy_attachment" "rds_monitoring" {
  count = var.monitoring_interval > 0 ? 1 : 0

  role       = aws_iam_role.rds_monitoring[0].name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonRDSEnhancedMonitoringRole"
}
