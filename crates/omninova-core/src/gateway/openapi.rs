//! OpenAPI Specification for RESTful API
//!
//! This module provides OpenAPI 3.0.3 specification for the agent REST API.

use serde_json::json;

/// OpenAPI 3.0.3 Specification for Agent API
pub fn get_openapi_spec() -> serde_json::Value {
    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "OmniNova Agent API",
            "description": "RESTful API for managing AI agents and interacting with them",
            "version": env!("CARGO_PKG_VERSION"),
            "contact": {
                "name": "OmniNova Team"
            }
        },
        "servers": [
            {
                "url": "/api",
                "description": "Relative server URL"
            }
        ],
        "tags": [
            {
                "name": "agents",
                "description": "Agent management operations"
            },
            {
                "name": "chat",
                "description": "Chat operations with agents"
            }
        ],
        "paths": {
            "/agents": {
                "get": {
                    "tags": ["agents"],
                    "summary": "List all agents",
                    "description": "Retrieve a paginated list of all agents",
                    "operationId": "listAgents",
                    "parameters": [
                        {
                            "name": "page",
                            "in": "query",
                            "description": "Page number (1-indexed)",
                            "required": false,
                            "schema": {
                                "type": "integer",
                                "default": 1,
                                "minimum": 1
                            }
                        },
                        {
                            "name": "perPage",
                            "in": "query",
                            "description": "Items per page (max 100)",
                            "required": false,
                            "schema": {
                                "type": "integer",
                                "default": 20,
                                "minimum": 1,
                                "maximum": 100
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Successful response with paginated agents",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/PaginatedAgentsResponse"
                                    }
                                }
                            }
                        },
                        "500": {
                            "description": "Internal server error",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiError"
                                    }
                                }
                            }
                        }
                    }
                },
                "post": {
                    "tags": ["agents"],
                    "summary": "Create a new agent",
                    "description": "Create a new AI agent with the specified configuration",
                    "operationId": "createAgent",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/CreateAgentRequest"
                                },
                                "examples": {
                                    "basic": {
                                        "summary": "Basic agent",
                                        "value": {
                                            "name": "My Assistant",
                                            "description": "A helpful AI assistant"
                                        }
                                    },
                                    "full": {
                                        "summary": "Full configuration",
                                        "value": {
                                            "name": "Research Bot",
                                            "description": "Specialized in research tasks",
                                            "domain": "research",
                                            "mbtiType": "INTJ",
                                            "systemPrompt": "You are a research assistant specialized in scientific literature.",
                                            "defaultProviderId": "openai-gpt4"
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Agent created successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/AgentResponse"
                                    }
                                }
                            }
                        },
                        "400": {
                            "description": "Validation error",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiError"
                                    }
                                }
                            }
                        },
                        "500": {
                            "description": "Internal server error",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiError"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/agents/{id}": {
                "get": {
                    "tags": ["agents"],
                    "summary": "Get agent by ID",
                    "description": "Retrieve a specific agent by its ID",
                    "operationId": "getAgent",
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "description": "Agent ID",
                            "required": true,
                            "schema": {
                                "type": "integer",
                                "format": "int64"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Successful response with agent details",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/AgentResponse"
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "Agent not found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiError"
                                    }
                                }
                            }
                        },
                        "500": {
                            "description": "Internal server error",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiError"
                                    }
                                }
                            }
                        }
                    }
                },
                "put": {
                    "tags": ["agents"],
                    "summary": "Update agent",
                    "description": "Update an existing agent's configuration",
                    "operationId": "updateAgent",
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "description": "Agent ID",
                            "required": true,
                            "schema": {
                                "type": "integer",
                                "format": "int64"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/UpdateAgentRequest"
                                },
                                "examples": {
                                    "updateName": {
                                        "summary": "Update name only",
                                        "value": {
                                            "name": "Updated Agent Name"
                                        }
                                    },
                                    "updateStatus": {
                                        "summary": "Change status",
                                        "value": {
                                            "status": "inactive"
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Agent updated successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/AgentResponse"
                                    }
                                }
                            }
                        },
                        "400": {
                            "description": "Validation error",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiError"
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "Agent not found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiError"
                                    }
                                }
                            }
                        },
                        "500": {
                            "description": "Internal server error",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiError"
                                    }
                                }
                            }
                        }
                    }
                },
                "delete": {
                    "tags": ["agents"],
                    "summary": "Delete agent",
                    "description": "Delete an agent by its ID",
                    "operationId": "deleteAgent",
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "description": "Agent ID",
                            "required": true,
                            "schema": {
                                "type": "integer",
                                "format": "int64"
                            }
                        }
                    ],
                    "responses": {
                        "204": {
                            "description": "Agent deleted successfully"
                        },
                        "404": {
                            "description": "Agent not found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiError"
                                    }
                                }
                            }
                        },
                        "500": {
                            "description": "Internal server error",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiError"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/agents/{id}/chat": {
                "post": {
                    "tags": ["chat"],
                    "summary": "Send a chat message",
                    "description": "Send a message to an agent and receive a synchronous response",
                    "operationId": "chatWithAgent",
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "description": "Agent ID",
                            "required": true,
                            "schema": {
                                "type": "integer",
                                "format": "int64"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/ChatRequest"
                                },
                                "examples": {
                                    "simple": {
                                        "summary": "Simple message",
                                        "value": {
                                            "message": "Hello, how can you help me?"
                                        }
                                    },
                                    "withSession": {
                                        "summary": "Continue conversation",
                                        "value": {
                                            "message": "Tell me more about that",
                                            "sessionId": 12345
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Chat response",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ChatResponse"
                                    }
                                }
                            }
                        },
                        "400": {
                            "description": "Validation error (e.g., empty message, inactive agent)",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiError"
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "Agent not found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiError"
                                    }
                                }
                            }
                        },
                        "500": {
                            "description": "Internal server error",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiError"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/agents/{id}/chat/stream": {
                "post": {
                    "tags": ["chat"],
                    "summary": "Stream chat response",
                    "description": "Send a message to an agent and receive a streaming response via Server-Sent Events (SSE)",
                    "operationId": "streamChatWithAgent",
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "description": "Agent ID",
                            "required": true,
                            "schema": {
                                "type": "integer",
                                "format": "int64"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/ChatRequest"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "SSE stream with chat response",
                            "content": {
                                "text/event-stream": {
                                    "schema": {
                                        "type": "string",
                                        "description": "Server-Sent Events stream with event types: start, delta, done, error"
                                    },
                                    "examples": {
                                        "stream": {
                                            "summary": "Example SSE stream",
                                            "value": "event: start\ndata: {}\n\nevent: delta\ndata: {\"delta\":\"Hello!\"}\n\nevent: done\ndata: {}\n\n"
                                        }
                                    }
                                }
                            }
                        },
                        "400": {
                            "description": "Validation error",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiError"
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "Agent not found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiError"
                                    }
                                }
                            }
                        },
                        "500": {
                            "description": "Internal server error",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiError"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        },
        "components": {
            "schemas": {
                "AgentResponse": {
                    "type": "object",
                    "required": ["success", "data"],
                    "properties": {
                        "success": {
                            "type": "boolean",
                            "example": true
                        },
                        "data": {
                            "$ref": "#/components/schemas/Agent"
                        }
                    }
                },
                "PaginatedAgentsResponse": {
                    "type": "object",
                    "required": ["success", "data", "pagination"],
                    "properties": {
                        "success": {
                            "type": "boolean",
                            "example": true
                        },
                        "data": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/Agent"
                            }
                        },
                        "pagination": {
                            "$ref": "#/components/schemas/Pagination"
                        }
                    }
                },
                "Agent": {
                    "type": "object",
                    "required": ["id", "agentUuid", "name", "status", "createdAt", "updatedAt"],
                    "properties": {
                        "id": {
                            "type": "integer",
                            "format": "int64",
                            "description": "Unique identifier",
                            "example": 1
                        },
                        "agentUuid": {
                            "type": "string",
                            "description": "UUID for external reference",
                            "example": "550e8400-e29b-41d4-a716-446655440000"
                        },
                        "name": {
                            "type": "string",
                            "description": "Agent name",
                            "example": "Research Assistant",
                            "maxLength": 100
                        },
                        "description": {
                            "type": "string",
                            "description": "Agent description",
                            "example": "An AI assistant specialized in research tasks"
                        },
                        "domain": {
                            "type": "string",
                            "description": "Domain/specialization",
                            "example": "research"
                        },
                        "mbtiType": {
                            "type": "string",
                            "description": "MBTI personality type",
                            "example": "INTJ",
                            "enum": ["INTJ", "INTP", "ENTJ", "ENTP", "INFJ", "INFP", "ENFJ", "ENFP", "ISTJ", "ISFJ", "ESTJ", "ESFJ", "ISTP", "ISFP", "ESTP", "ESFP"]
                        },
                        "systemPrompt": {
                            "type": "string",
                            "description": "Custom system prompt"
                        },
                        "status": {
                            "type": "string",
                            "description": "Agent status",
                            "enum": ["active", "inactive"],
                            "example": "active"
                        },
                        "defaultProviderId": {
                            "type": "string",
                            "description": "Default LLM provider ID"
                        },
                        "createdAt": {
                            "type": "integer",
                            "format": "int64",
                            "description": "Creation timestamp (Unix epoch)"
                        },
                        "updatedAt": {
                            "type": "integer",
                            "format": "int64",
                            "description": "Last update timestamp (Unix epoch)"
                        }
                    }
                },
                "Pagination": {
                    "type": "object",
                    "required": ["page", "perPage", "total", "totalPages"],
                    "properties": {
                        "page": {
                            "type": "integer",
                            "description": "Current page number (1-indexed)",
                            "example": 1
                        },
                        "perPage": {
                            "type": "integer",
                            "description": "Items per page",
                            "example": 20
                        },
                        "total": {
                            "type": "integer",
                            "format": "int64",
                            "description": "Total number of items",
                            "example": 100
                        },
                        "totalPages": {
                            "type": "integer",
                            "description": "Total number of pages",
                            "example": 5
                        }
                    }
                },
                "CreateAgentRequest": {
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Agent name (required, max 100 characters)",
                            "example": "My Assistant",
                            "maxLength": 100,
                            "minLength": 1
                        },
                        "description": {
                            "type": "string",
                            "description": "Agent description"
                        },
                        "domain": {
                            "type": "string",
                            "description": "Domain/specialization"
                        },
                        "mbtiType": {
                            "type": "string",
                            "description": "MBTI personality type",
                            "enum": ["INTJ", "INTP", "ENTJ", "ENTP", "INFJ", "INFP", "ENFJ", "ENFP", "ISTJ", "ISFJ", "ESTJ", "ESFJ", "ISTP", "ISFP", "ESTP", "ESFP"]
                        },
                        "systemPrompt": {
                            "type": "string",
                            "description": "Custom system prompt"
                        },
                        "defaultProviderId": {
                            "type": "string",
                            "description": "Default LLM provider ID"
                        }
                    }
                },
                "UpdateAgentRequest": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "New agent name",
                            "maxLength": 100
                        },
                        "description": {
                            "type": "string",
                            "description": "New description"
                        },
                        "domain": {
                            "type": "string",
                            "description": "New domain"
                        },
                        "mbtiType": {
                            "type": "string",
                            "description": "New MBTI type",
                            "enum": ["INTJ", "INTP", "ENTJ", "ENTP", "INFJ", "INFP", "ENFJ", "ENFP", "ISTJ", "ISFJ", "ESTJ", "ESFJ", "ISTP", "ISFP", "ESTP", "ESFP"]
                        },
                        "systemPrompt": {
                            "type": "string",
                            "description": "New system prompt"
                        },
                        "status": {
                            "type": "string",
                            "description": "New status",
                            "enum": ["active", "inactive"]
                        },
                        "defaultProviderId": {
                            "type": "string",
                            "description": "New default provider ID"
                        }
                    }
                },
                "ChatRequest": {
                    "type": "object",
                    "required": ["message"],
                    "properties": {
                        "message": {
                            "type": "string",
                            "description": "Message to send to the agent",
                            "example": "What can you help me with?",
                            "minLength": 1
                        },
                        "sessionId": {
                            "type": "integer",
                            "format": "int64",
                            "description": "Optional session ID to continue an existing conversation"
                        },
                        "context": {
                            "$ref": "#/components/schemas/ChatContext"
                        }
                    }
                },
                "ChatContext": {
                    "type": "object",
                    "properties": {
                        "includeMemory": {
                            "type": "boolean",
                            "description": "Whether to include memory context",
                            "default": true
                        },
                        "maxTokens": {
                            "type": "integer",
                            "description": "Maximum tokens for the response",
                            "example": 2048
                        }
                    }
                },
                "ChatResponse": {
                    "type": "object",
                    "required": ["success", "data"],
                    "properties": {
                        "success": {
                            "type": "boolean",
                            "example": true
                        },
                        "data": {
                            "type": "object",
                            "required": ["response", "sessionId", "messageId"],
                            "properties": {
                                "response": {
                                    "type": "string",
                                    "description": "Agent's response text"
                                },
                                "sessionId": {
                                    "type": "integer",
                                    "format": "int64",
                                    "description": "Session ID (existing or newly created)"
                                },
                                "messageId": {
                                    "type": "integer",
                                    "format": "int64",
                                    "description": "Message ID of the response"
                                }
                            }
                        }
                    }
                },
                "ApiError": {
                    "type": "object",
                    "required": ["success", "error"],
                    "properties": {
                        "success": {
                            "type": "boolean",
                            "example": false
                        },
                        "error": {
                            "type": "object",
                            "required": ["code", "message"],
                            "properties": {
                                "code": {
                                    "type": "string",
                                    "description": "Error code",
                                    "example": "NOT_FOUND",
                                    "enum": ["NOT_FOUND", "VALIDATION_ERROR", "INTERNAL_ERROR", "SERVICE_UNAVAILABLE"]
                                },
                                "message": {
                                    "type": "string",
                                    "description": "Human-readable error message",
                                    "example": "Agent with id 123 not found"
                                },
                                "details": {
                                    "type": "object",
                                    "description": "Additional error details"
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openapi_spec_is_valid_json() {
        let spec = get_openapi_spec();
        assert!(spec.is_object());
        assert_eq!(spec["openapi"], "3.0.3");
        assert!(spec["info"]["title"].is_string());
        assert!(spec["paths"].is_object());
    }

    #[test]
    fn test_openapi_spec_has_required_fields() {
        let spec = get_openapi_spec();
        assert!(spec.get("openapi").is_some());
        assert!(spec.get("info").is_some());
        assert!(spec.get("paths").is_some());
        assert!(spec.get("components").is_some());
    }

    #[test]
    fn test_openapi_spec_has_agent_endpoints() {
        let spec = get_openapi_spec();
        let paths = &spec["paths"];

        assert!(paths.get("/agents").is_some());
        assert!(paths.get("/agents/{id}").is_some());
        assert!(paths.get("/agents/{id}/chat").is_some());
        assert!(paths.get("/agents/{id}/chat/stream").is_some());
    }

    #[test]
    fn test_openapi_spec_has_schemas() {
        let spec = get_openapi_spec();
        let schemas = &spec["components"]["schemas"];

        assert!(schemas.get("Agent").is_some());
        assert!(schemas.get("AgentResponse").is_some());
        assert!(schemas.get("CreateAgentRequest").is_some());
        assert!(schemas.get("UpdateAgentRequest").is_some());
        assert!(schemas.get("ChatRequest").is_some());
        assert!(schemas.get("ChatResponse").is_some());
        assert!(schemas.get("ApiError").is_some());
    }
}