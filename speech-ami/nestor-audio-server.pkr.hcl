packer {
  required_plugins {
    amazon = {
      version = ">= 1.0.0"
      source  = "github.com/hashicorp/amazon"
    }
  }
}

variable "aws_region" {
  type    = string
  default = "eu-west-3"
}

variable "instance_type" {
  type    = string
  default = "t3.medium"  # Build instance (GPU not needed for AMI creation)
  description = "Instance type for building the AMI. Use t3.medium for cost efficiency - GPU is only needed at runtime."
}

variable "ami_name_prefix" {
  type    = string
  default = "nestor-audio-server"
}

variable "source_ami_owner" {
  type    = string
  default = "099720109477" # Canonical (Ubuntu)
}

variable "iam_instance_profile" {
  type        = string
  description = "IAM instance profile to attach to the builder instance"
  default     = "" # Leave empty if no profile is needed
}

variable "vpc_id" {
  type        = string
  description = "VPC ID where the builder instance will be launched"
  default     = ""
}

variable "subnet_id" {
  type        = string
  description = "Subnet ID where the builder instance will be launched"
  default     = ""
}

variable "associate_public_ip_address" {
  type        = bool
  description = "Whether to associate a public IP address with the builder instance"
  default     = true
}

locals {
  timestamp = regex_replace(timestamp(), "[- TZ:]", "")
  ami_name  = "${var.ami_name_prefix}-${local.timestamp}"
}

source "amazon-ebs" "nestor_audio" {
  ami_name                    = local.ami_name
  instance_type               = var.instance_type
  region                      = var.aws_region
  iam_instance_profile        = var.iam_instance_profile
  vpc_id                      = var.vpc_id
  subnet_id                   = var.subnet_id
  associate_public_ip_address = var.associate_public_ip_address
  
  # Increase volume size for Docker build (Whisper + PyTorch + CUDA are large)
  launch_block_device_mappings {
    device_name = "/dev/sda1"
    volume_size = 30
    volume_type = "gp3"
    delete_on_termination = true
  }
  
  source_ami_filter {
    filters = {
      name                = "ubuntu/images/hvm-ssd/ubuntu-jammy-22.04-amd64-server-*"
      root-device-type    = "ebs"
      virtualization-type = "hvm"
    }
    most_recent = true
    owners      = [var.source_ami_owner]
  }
  
  ssh_username = "ubuntu"
  
  tags = {
    Name        = local.ami_name
    Environment = "production"
    Application = "nestor-audio-server"
    ManagedBy   = "packer"
    BuildDate   = local.timestamp
  }
  
  run_tags = {
    Name = "packer-builder-nestor-audio-server"
  }
}

build {
  sources = ["source.amazon-ebs.nestor_audio"]
  
  # Update system packages
  provisioner "shell" {
    inline = [
      "echo 'Waiting for cloud-init to complete...'",
      "sudo cloud-init status --wait",
      "echo 'Updating system packages...'",
      "sudo apt-get update",
      "sudo apt-get upgrade -y",
    ]
  }
  
  # Install Docker
  provisioner "shell" {
    inline = [
      "echo 'Installing Docker...'",
      "curl -fsSL https://get.docker.com -o get-docker.sh",
      "sudo sh get-docker.sh",
      "sudo usermod -aG docker ubuntu",
      "rm get-docker.sh",
    ]
  }
  
  # Install NVIDIA Container Toolkit for GPU support
  provisioner "shell" {
    inline = [
      "echo 'Installing NVIDIA Container Toolkit...'",
      "curl -fsSL https://nvidia.github.io/libnvidia-container/gpgkey | sudo gpg --dearmor -o /usr/share/keyrings/nvidia-container-toolkit-keyring.gpg",
      "curl -s -L https://nvidia.github.io/libnvidia-container/stable/deb/nvidia-container-toolkit.list | sed 's#deb https://#deb [signed-by=/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg] https://#g' | sudo tee /etc/apt/sources.list.d/nvidia-container-toolkit.list",
      "sudo apt-get update",
      "sudo apt-get install -y nvidia-container-toolkit",
      "sudo nvidia-ctk runtime configure --runtime=docker",
      "sudo systemctl restart docker",
    ]
  }
  
  # Install AWS CLI
  provisioner "shell" {
    inline = [
      "echo 'Installing AWS CLI...'",
      "sudo apt-get install -y unzip",
      "curl 'https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip' -o 'awscliv2.zip'",
      "unzip awscliv2.zip",
      "sudo ./aws/install",
      "rm -rf awscliv2.zip aws",
    ]
  }
  
  # Copy Dockerfile and application code
  provisioner "file" {
    source      = "Dockerfile"
    destination = "/tmp/Dockerfile"
  }
  
  provisioner "file" {
    source      = "process_audio.py"
    destination = "/tmp/process_audio.py"
  }
  
  # Build Docker image
  provisioner "shell" {
    inline = [
      "echo 'Building Whisper processor Docker image...'",
      "cd /tmp",
      "sudo docker build -t nestor-whisper-processor:latest .",
      "rm Dockerfile process_audio.py",
    ]
  }
  
  # Install AWS CloudWatch agent
  provisioner "shell" {
    inline = [
      "echo 'Installing CloudWatch agent...'",
      "wget https://s3.amazonaws.com/amazoncloudwatch-agent/ubuntu/amd64/latest/amazon-cloudwatch-agent.deb",
      "sudo dpkg -i amazon-cloudwatch-agent.deb",
      "rm amazon-cloudwatch-agent.deb",
    ]
  }
  
  # Clean up
  provisioner "shell" {
    inline = [
      "echo 'Cleaning up...'",
      "sudo apt-get autoremove -y",
      "sudo apt-get clean",
      "sudo rm -rf /var/lib/apt/lists/*",
      "sudo rm -rf /tmp/*",
      "sudo rm -rf /var/tmp/*",
      "sudo find /root -name '.bash_history' -delete || true",
      "sudo find /home -name '.bash_history' -delete || true",
    ]
  }
  
  post-processor "manifest" {
    output     = "manifest.json"
    strip_path = true
  }
}
