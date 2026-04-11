# API Documentation for Ferrumec Catalog

## Overview
This document provides comprehensive API documentation for the Ferrumec Catalog.

## Base URL
`https://api.ferrumec.com/v1`

## Authentication
All API requests require authentication. Use a Bearer token in the header.

### Example:
```http
Authorization: Bearer YOUR_ACCESS_TOKEN
```

## Endpoints

### 1. **Get All Items**
- **Endpoint:** `/items`
- **Method:** `GET`
- **Request:** No parameters
- **Response:** 
  - **Status 200:** Returns an array of items.
  - **Data Model:** 
    ```json
    [
      {
        "id": "string",
        "name": "string",
        "description": "string",
        "price": "number"
      }
    ]
    ```

### 2. **Get Item by ID**
- **Endpoint:** `/items/{id}`
- **Method:** `GET`
- **Parameters:**
  - `id`: Unique identifier for the item
- **Response:** 
  - **Status 200:** Returns the item details.
  - **Data Model:** 
    ```json
    {
      "id": "string",
      "name": "string",
      "description": "string",
      "price": "number"
    }
    ```

### 3. **Create New Item**
- **Endpoint:** `/items`
- **Method:** `POST`
- **Request Body:**
  - **Data Model:** 
    ```json
    {
      "name": "string",
      "description": "string",
      "price": "number"
    }
    ```
- **Response:** 
  - **Status 201:** Returns the created item.

### 4. **Update Item**
- **Endpoint:** `/items/{id}`
- **Method:** `PUT`
- **Parameters:**
  - `id`: Unique identifier for the item
- **Request Body:** Same as Create New Item
- **Response:** 
  - **Status 200:** Returns the updated item.

### 5. **Delete Item**
- **Endpoint:** `/items/{id}`
- **Method:** `DELETE`
- **Parameters:**
  - `id`: Unique identifier for the item
- **Response:** 
  - **Status 204:** No content, item deleted successfully.

## Error Handling

Common error responses:

- **400 Bad Request:** Invalid input parameters.
- **401 Unauthorized:** Authentication failed.
- **404 Not Found:** Resource not found.
- **500 Internal Server Error:** Unexpected error occurred.

## Usage Examples

### Example 1: Get All Items
```http
GET /items HTTP/1.1
Host: api.ferrumec.com
Authorization: Bearer YOUR_ACCESS_TOKEN
```

### Example 2: Create New Item
```http
POST /items HTTP/1.1
Host: api.ferrumec.com
Authorization: Bearer YOUR_ACCESS_TOKEN
Content-Type: application/json

{
  "name": "New Widget",
  "description": "A brand new widget.",
  "price": 19.99
}
```