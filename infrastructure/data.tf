# Data source to find the Whisper AMI by alias tag
data "aws_ami" "whisper_by_alias" {
  count       = var.whisper_ami_id == "" ? 1 : 0
  most_recent = true
  owners      = ["self"]

  filter {
    name   = "tag:AmiAlias"
    values = ["nestor-whisper-latest"]
  }

  filter {
    name   = "state"
    values = ["available"]
  }
}

# Fallback to latest AMI by name if alias not found
data "aws_ami" "whisper_by_name" {
  count       = var.whisper_ami_id == "" ? 1 : 0
  most_recent = true
  owners      = ["self"]

  filter {
    name   = "name"
    values = ["nestor-audio-server-*"]
  }

  filter {
    name   = "state"
    values = ["available"]
  }
}

# Use provided AMI ID, alias AMI, or fallback to latest by name
locals {
  whisper_ami_id = var.whisper_ami_id != "" ? var.whisper_ami_id : (
    try(data.aws_ami.whisper_by_alias[0].id, null) != null
    ? data.aws_ami.whisper_by_alias[0].id
    : try(data.aws_ami.whisper_by_name[0].id, null)
  )
}
