"""
catalog_client.py
-----------------
Python client for the Ferrumec catalog service.

Endpoints covered
-----------------
GET  /                          -> index()         HTML catalog page
GET  /products                  -> list_products()
GET  /products/{id}             -> get_product()
GET  /products/slug/{slug}      -> get_product_by_slug()
POST /products                  -> create_product()  (requires auth token)
PATCH /products/{id}            -> update_product()  (requires auth token)
"""

from __future__ import annotations

import dataclasses
from dataclasses import dataclass, field
from typing import Optional
import requests
from requests import Response, Session


# ---------------------------------------------------------------------------
# Exceptions
# ---------------------------------------------------------------------------

class CatalogClientError(Exception):
    """Base exception for all catalog client errors."""


class NotFoundError(CatalogClientError):
    """Raised when the service returns HTTP 404."""


class AuthorizationError(CatalogClientError):
    """Raised when the service returns HTTP 401 or 403."""


class ServerError(CatalogClientError):
    """Raised when the service returns HTTP 5xx."""


# ---------------------------------------------------------------------------
# Data models (mirrors Rust structs)
# ---------------------------------------------------------------------------

@dataclass
class Product:
    id: str
    name: str
    slug: str
    sku: str
    price: float
    category: str
    created_at: int                  # Unix microseconds
    description: Optional[str] = None

    @classmethod
    def from_dict(cls, data: dict) -> "Product":
        return cls(
            id=data["id"],
            name=data["name"],
            slug=data["slug"],
            sku=data["sku"],
            price=data["price"],
            category=data["category"],
            created_at=data["created_at"],
            description=data.get("description"),
        )


@dataclass
class CreateProductDto:
    name: str
    price: float
    category: str
    sku: str
    qty: int
    description: Optional[str] = None

    def to_dict(self) -> dict:
        d = dataclasses.asdict(self)
        # Drop None values — the server treats absent fields as None.
        return {k: v for k, v in d.items() if v is not None}


@dataclass
class UpdateProductDto:
    name: Optional[str] = None
    description: Optional[str] = None
    price: Optional[float] = None
    category: Optional[str] = None
    qty: Optional[int] = None
    sku: Optional[str] = None

    def to_dict(self) -> dict:
        d = dataclasses.asdict(self)
        return {k: v for k, v in d.items() if v is not None}

    def is_empty(self) -> bool:
        return not self.to_dict()


@dataclass
class ProductQuery:
    q: Optional[str] = None
    min_price: Optional[float] = None
    max_price: Optional[float] = None
    category: Optional[str] = None
    limit: Optional[int] = None
    offset: Optional[int] = None

    def to_params(self) -> dict:
        """Serialise to query-string params, omitting unset fields."""
        params: dict = {}
        if self.q is not None:
            params["q"] = self.q
        if self.min_price is not None:
            params["min_price"] = str(self.min_price)
        if self.max_price is not None:
            params["max_price"] = str(self.max_price)
        if self.category is not None:
            params["category"] = self.category
        if self.limit is not None:
            params["limit"] = str(self.limit)
        if self.offset is not None:
            params["offset"] = str(self.offset)
        return params


# ---------------------------------------------------------------------------
# Client
# ---------------------------------------------------------------------------

class CatalogClient:
    """
    HTTP client for the Ferrumec catalog service.

    Parameters
    ----------
    base_url:
        Root URL of the catalog namespace, e.g. ``"http://localhost:8080/catalog"``.
        Do not include a trailing slash.
    token:
        Optional Bearer token for authenticated endpoints (create / update).
        Can also be supplied later via :meth:`set_token`.
    timeout:
        Request timeout in seconds (default: 10).
    session:
        Optional pre-configured :class:`requests.Session`. Useful for injecting
        test doubles or custom TLS/proxy settings.

    Usage
    -----
    >>> client = CatalogClient("http://localhost:8080/catalog", token="<jwt>")
    >>> products = client.list_products(ProductQuery(category="electronics", limit=10))
    >>> new_product = client.create_product(
    ...     CreateProductDto(name="Widget", price=9.99, category="gadgets", sku="WDG-001", qty=50)
    ... )
    """

    def __init__(
        self,
        base_url: str,
        token: Optional[str] = None,
        timeout: int = 10,
        session: Optional[Session] = None,
    ) -> None:
        self._base_url = base_url.rstrip("/")
        self._timeout = timeout
        self._session = session or requests.Session()
        if token:
            self.set_token(token)

    # ------------------------------------------------------------------
    # Auth helpers
    # ------------------------------------------------------------------

    def set_token(self, token: str) -> None:
        """Set or replace the Bearer token used for authenticated requests."""
        self._session.headers.update({"Authorization": f"Bearer {token}"})

    def clear_token(self) -> None:
        """Remove the Bearer token (revert to unauthenticated requests)."""
        self._session.headers.pop("Authorization", None)

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def get_catalog_page(self, query: Optional[ProductQuery] = None) -> str:
        """
        Fetch the server-rendered HTML catalog page.

        Maps to: GET /

        Returns
        -------
        str
            Raw HTML body of the rendered catalog page.
        """
        params = query.to_params() if query else {}
        response = self._get("/", params=params)
        return response.text

    def list_products(self, query: Optional[ProductQuery] = None) -> list[Product]:
        """
        List products, with optional filtering and pagination.

        Maps to: GET /products

        Parameters
        ----------
        query:
            :class:`ProductQuery` with any combination of ``q``, ``category``,
            ``min_price``, ``max_price``, ``limit``, and ``offset``.

        Returns
        -------
        list[Product]
        """
        params = query.to_params() if query else {}
        response = self._get("/products", params=params)
        return [Product.from_dict(p) for p in response.json()]

    def get_product(self, product_id: str) -> Product:
        """
        Fetch a single product by its UUID.

        Maps to: GET /products/{id}

        Raises
        ------
        NotFoundError
            If no product with that ID exists.
        """
        response = self._get(f"/products/{product_id}")
        return Product.from_dict(response.json())

    def get_product_by_slug(self, slug: str) -> Product:
        """
        Fetch a single product by its URL slug.

        Maps to: GET /products/slug/{slug}

        Raises
        ------
        NotFoundError
            If no product with that slug exists.
        """
        response = self._get(f"/products/slug/{slug}")
        return Product.from_dict(response.json())

    def create_product(self, dto: CreateProductDto) -> Product:
        """
        Create a new product. Requires a valid Bearer token.

        Maps to: POST /products

        Parameters
        ----------
        dto:
            :class:`CreateProductDto` with all required fields populated.

        Returns
        -------
        Product
            The newly created product as returned by the server (includes generated
            ``id``, ``slug``, and ``created_at``).

        Raises
        ------
        AuthorizationError
            If the token is missing, expired, or lacks the ``create_product`` permission.
        """
        response = self._post("/products", json=dto.to_dict())
        return Product.from_dict(response.json())

    def update_product(self, product_id: str, dto: UpdateProductDto) -> None:
        """
        Partially update an existing product. Requires a valid Bearer token.

        Maps to: PATCH /products/{id}

        Only fields set to a non-``None`` value on ``dto`` are sent.
        Calling this with an entirely empty ``dto`` raises :class:`ValueError`
        to avoid a useless round-trip.

        Raises
        ------
        ValueError
            If ``dto`` has no fields set.
        NotFoundError
            If no product with that ID exists.
        AuthorizationError
            If the token is missing, expired, or lacks the required permission.
        """
        if dto.is_empty():
            raise ValueError("UpdateProductDto has no fields set — nothing to update.")
        self._patch(f"/products/{product_id}", json=dto.to_dict())

    # ------------------------------------------------------------------
    # Internal HTTP helpers
    # ------------------------------------------------------------------

    def _url(self, path: str) -> str:
        return f"{self._base_url}{path}"

    def _raise_for_status(self, response: Response) -> None:
        """Translate HTTP error codes into typed exceptions."""
        code = response.status_code
        if code == 404:
            body = _safe_json(response)
            raise NotFoundError(body.get("error", "Resource not found"))
        if code in (401, 403):
            raise AuthorizationError(
                f"HTTP {code}: insufficient permissions or missing token."
            )
        if code >= 500:
            raise ServerError(f"HTTP {code}: server error from catalog service.")
        # For any other 4xx let requests raise its own HTTPError.
        response.raise_for_status()

    def _get(self, path: str, **kwargs) -> Response:
        r = self._session.get(self._url(path), timeout=self._timeout, **kwargs)
        self._raise_for_status(r)
        return r

    def _post(self, path: str, **kwargs) -> Response:
        r = self._session.post(self._url(path), timeout=self._timeout, **kwargs)
        self._raise_for_status(r)
        return r

    def _patch(self, path: str, **kwargs) -> Response:
        r = self._session.patch(self._url(path), timeout=self._timeout, **kwargs)
        self._raise_for_status(r)
        return r


# ---------------------------------------------------------------------------
# Utilities
# ---------------------------------------------------------------------------

def _safe_json(response: Response) -> dict:
    try:
        return response.json()
    except Exception:
        return {}
