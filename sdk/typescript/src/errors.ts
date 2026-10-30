export class AxiomError extends Error {
  readonly code: string;
  readonly details?: unknown;
  constructor(message: string, code = "axiom_error", details?: unknown) {
    super(message);
    this.name = "AxiomError";
    this.code = code;
    this.details = details;
  }
}

export class RuntimeConnectionError extends AxiomError {
  readonly statusCode?: number;
  readonly body?: unknown;
  constructor(message: string, opts: { statusCode?: number; body?: unknown } = {}) {
    super(message, "runtime_connection", opts.body);
    this.name = "RuntimeConnectionError";
    this.statusCode = opts.statusCode;
    this.body = opts.body;
  }
}

export class RuntimeRejectedError extends AxiomError {
  readonly statusCode: number;
  constructor(code: string, message: string, statusCode: number, details?: unknown) {
    super(`${code}: ${message}`, code, details);
    this.name = "RuntimeRejectedError";
    this.statusCode = statusCode;
  }
}

export class ToolError extends AxiomError {
  readonly retryable: boolean;
  constructor(message: string, opts: { code?: string; retryable?: boolean; details?: unknown } = {}) {
    super(message, opts.code ?? "tool_error", opts.details);
    this.name = "ToolError";
    this.retryable = opts.retryable ?? false;
  }
}

export class RetryableToolError extends ToolError {
  constructor(message: string, opts: { code?: string; details?: unknown } = {}) {
    super(message, { code: opts.code ?? "transient_error", retryable: true, details: opts.details });
    this.name = "RetryableToolError";
  }
}

export class ModelAdapterError extends AxiomError {
  readonly retryable: boolean;
  constructor(message: string, opts: { code?: string; retryable?: boolean; details?: unknown } = {}) {
    super(message, opts.code ?? "provider_error", opts.details);
    this.name = "ModelAdapterError";
    this.retryable = opts.retryable ?? false;
  }
}

export class ConfigurationError extends AxiomError {
  constructor(message: string) {
    super(message, "configuration");
    this.name = "ConfigurationError";
  }
}

export class CancellationError extends AxiomError {
  constructor(message = "operation cancelled") {
    super(message, "cancelled");
    this.name = "CancellationError";
  }
}

export class PermissionDeniedError extends ToolError {
  constructor(message: string, details?: unknown) {
    super(message, { code: "permission_denied", retryable: false, details });
    this.name = "PermissionDeniedError";
  }
}

export class SchemaValidationError extends ToolError {
  constructor(message: string, details?: unknown) {
    super(message, { code: "invalid_arguments", retryable: false, details });
    this.name = "SchemaValidationError";
  }
}
