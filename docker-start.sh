#!/bin/bash

# Rusty Kaspa Docker Startup Script
# This script helps you quickly start Rusty Kaspa nodes in Docker

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_header() {
    echo -e "${BLUE}================================${NC}"
    echo -e "${BLUE}  Rusty Kaspa Docker Setup${NC}"
    echo -e "${BLUE}================================${NC}"
}

# Function to check if Docker is running
check_docker() {
    if ! docker info > /dev/null 2>&1; then
        print_error "Docker is not running. Please start Docker and try again."
        exit 1
    fi
}

# Function to check if docker-compose is available
check_docker_compose() {
    if ! command -v docker-compose > /dev/null 2>&1; then
        print_error "docker-compose is not installed. Please install docker-compose and try again."
        exit 1
    fi
}

# Function to setup environment file
setup_env() {
    if [ ! -f .env ]; then
        if [ -f env.example ]; then
            print_status "Creating .env file from env.example..."
            cp env.example .env
            print_status ".env file created. You can edit it to customize your configuration."
        else
            print_warning "env.example not found. Creating basic .env file..."
            cat > .env << EOF
# Rusty Kaspa Docker Configuration
LOG_LEVEL=INFO
ENABLE_UTXO_INDEX=true
ENABLE_PERF_METRICS=false
OUTBOUND_TARGET=8
INBOUND_LIMIT=128
RPC_MAX_CLIENTS=128
WRPC_ENCODING=borsh
GRAFANA_PASSWORD=admin
EOF
        fi
    else
        print_status ".env file already exists."
    fi
}

# Function to show usage
show_usage() {
    echo "Usage: $0 [OPTIONS] [SERVICE]"
    echo ""
    echo "Options:"
    echo "  -h, --help          Show this help message"
    echo "  -b, --build         Build images before starting"
    echo "  -d, --detach        Run in background"
    echo "  -m, --monitoring    Include monitoring services (Prometheus + Grafana)"
    echo "  -c, --clean         Clean up containers and volumes"
    echo ""
    echo "Services:"
    echo "  mainnet             Start mainnet node"
    echo "  testnet             Start testnet node"
    echo "  devnet              Start devnet node"
    echo "  all                 Start all nodes"
    echo ""
    echo "Examples:"
    echo "  $0 mainnet                    # Start mainnet node with wRPC proxy"
    echo "  $0 -d testnet                 # Start testnet node in background"
    echo "  $0 -b -m mainnet              # Build and start mainnet with monitoring"
    echo "  $0 -c                         # Clean up everything"
    echo ""
    echo "Profile-based usage:"
    echo "  docker-compose --profile mainnet up -d     # Start mainnet only"
    echo "  docker-compose --profile testnet up -d     # Start testnet only"
    echo "  docker-compose --profile devnet up -d      # Start devnet only"
    echo "  docker-compose --profile monitoring up -d  # Start monitoring only"
}

# Function to build images
build_images() {
    print_status "Building Docker images..."
    docker-compose build --no-cache
    print_status "Build completed!"
}

# Function to start services
start_services() {
    local service=$1
    local detach=$2
    local monitoring=$3
    
    local compose_cmd="docker-compose"
    local profiles=""
    
    # Add network profile
    case $service in
        "mainnet")
            profiles="mainnet"
            ;;
        "testnet")
            profiles="testnet"
            ;;
        "devnet")
            profiles="devnet"
            ;;
        "all")
            profiles="mainnet,testnet,devnet"
            ;;
        *)
            print_error "Unknown service: $service"
            show_usage
            exit 1
            return
            ;;
    esac
    
    # Add monitoring profile if requested
    if [ "$monitoring" = "true" ]; then
        if [ -n "$profiles" ]; then
            profiles="$profiles,monitoring"
        else
            profiles="monitoring"
        fi
    fi
    
    # Build compose command
    if [ -n "$profiles" ]; then
        compose_cmd="$compose_cmd --profile $profiles"
    fi
    
    if [ "$detach" = "true" ]; then
        compose_cmd="$compose_cmd up -d"
    else
        compose_cmd="$compose_cmd up"
    fi
    
    case $service in
        "mainnet")
            print_status "Starting mainnet node with wRPC proxy..."
            ;;
        "testnet")
            print_status "Starting testnet node with wRPC proxy..."
            ;;
        "devnet")
            print_status "Starting devnet node..."
            ;;
        "all")
            print_status "Starting all nodes..."
            ;;
    esac
    
    print_status "Running: $compose_cmd"
    $compose_cmd
}

# Function to clean up
cleanup() {
    print_status "Cleaning up all containers and volumes..."
    docker-compose --profile mainnet,testnet,devnet,monitoring down -v
    print_status "Cleanup completed!"
}

# Function to show status
show_status() {
    print_status "Current Docker containers status:"
    docker-compose ps --all
    echo ""
    print_status "Port information:"
    echo "  Mainnet: gRPC 16110, wRPC Borsh 17110, wRPC JSON 18110, P2P 16111"
    echo "  Testnet: gRPC 16210, wRPC Borsh 17210, wRPC JSON 18210, P2P 16211"
    echo "  Devnet:  gRPC 16610, wRPC Borsh 17610, wRPC JSON 18610, P2P 16611"
    echo "  Monitoring: Prometheus 9090, Grafana 3000"
    echo ""
    print_status "Available profiles:"
    echo "  mainnet    - Mainnet node with wRPC proxy"
    echo "  testnet    - Testnet node with wRPC proxy"
    echo "  devnet     - Devnet node"
    echo "  monitoring - Prometheus and Grafana"
}

# Main script
main() {
    print_header
    
    # Parse command line arguments
    local build=false
    local detach=false
    local monitoring=false
    local cleanup_flag=false
    local service=""
    
    while [[ $# -gt 0 ]]; do
        case $1 in
            -h|--help)
                show_usage
                exit 0
                ;;
            -b|--build)
                build=true
                shift
                ;;
            -d|--detach)
                detach=true
                shift
                ;;
            -m|--monitoring)
                monitoring=true
                shift
                ;;
            -c|--clean)
                cleanup_flag=true
                shift
                ;;
            mainnet|testnet|devnet|all)
                service=$1
                shift
                ;;
            *)
                print_error "Unknown option: $1"
                show_usage
                exit 1
                ;;
        esac
    done
    
    # Check prerequisites
    check_docker
    check_docker_compose
    
    # Setup environment
    setup_env
    
    # Handle cleanup
    if [ "$cleanup_flag" = "true" ]; then
        cleanup
        exit 0
    fi
    
    # Handle build
    if [ "$build" = "true" ]; then
        build_images
    fi
    
    # Handle service start
    if [ -n "$service" ]; then
        start_services "$service" "$detach" "$monitoring"
        
        if [ "$detach" = "true" ]; then
            echo ""
            show_status
        fi
    else
        print_warning "No service specified. Use --help for usage information."
        show_usage
        exit 1
    fi
}

# Run main function with all arguments
main "$@" 