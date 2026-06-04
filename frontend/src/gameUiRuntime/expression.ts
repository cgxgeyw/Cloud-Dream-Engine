type GameUiExpressionScope = Record<string, unknown>;

type Token =
  | { type: "identifier"; value: string }
  | { type: "string"; value: string }
  | { type: "number"; value: number }
  | { type: "boolean"; value: boolean }
  | { type: "null"; value: null }
  | { type: "operator"; value: "==" | "!=" | ">" | ">=" | "<" | "<=" | "&&" | "||" }
  | { type: "paren"; value: "(" | ")" };

type OperatorToken = Extract<Token, { type: "operator" }>;
type ComparisonOperator = "==" | "!=" | ">" | ">=" | "<" | "<=";

export function evaluateGameUiExpression(
  expression: string,
  scope: GameUiExpressionScope,
): boolean {
  const tokens = tokenizeGameUiExpression(expression);
  if (tokens.length === 0) {
    return false;
  }

  const parser = createParser(tokens, scope);
  const result = parser.parseExpression();
  if (!parser.isDone()) {
    throw new Error(`Unexpected token in expression: ${expression}`);
  }

  return Boolean(result);
}

function createParser(tokens: Token[], scope: GameUiExpressionScope) {
  let index = 0;

  function peek(): Token | undefined {
    return tokens[index];
  }

  function consume(): Token {
    const token = tokens[index];
    if (!token) {
      throw new Error("Unexpected end of expression.");
    }
    index += 1;
    return token;
  }

  function parseExpression(): unknown {
    return parseOr();
  }

  function parseOr(): unknown {
    let left = parseAnd();
    while (peek()?.type === "operator" && peek()?.value === "||") {
      consume();
      const right = parseAnd();
      left = Boolean(left) || Boolean(right);
    }
    return left;
  }

  function parseAnd(): unknown {
    let left = parseComparison();
    while (peek()?.type === "operator" && peek()?.value === "&&") {
      consume();
      const right = parseComparison();
      left = Boolean(left) && Boolean(right);
    }
    return left;
  }

  function parseComparison(): unknown {
    let left = parsePrimary();

    while (isComparisonToken(peek())) {
      const operator = (consume() as OperatorToken).value as ComparisonOperator;
      const right = parsePrimary();
      left = applyComparison(left, operator, right);
    }

    return left;
  }

  function parsePrimary(): unknown {
    const token = consume();

    if (token.type === "paren" && token.value === "(") {
      const value = parseExpression();
      const next = consume();
      if (next.type !== "paren" || next.value !== ")") {
        throw new Error("Expected closing parenthesis.");
      }
      return value;
    }

    if (token.type === "identifier") {
      return resolveExpressionPath(scope, token.value);
    }

    if (token.type === "string" || token.type === "number" || token.type === "boolean") {
      return token.value;
    }

    if (token.type === "null") {
      return null;
    }

    throw new Error("Unexpected token in expression.");
  }

  return {
    parseExpression,
    isDone: () => index >= tokens.length,
  };
}

function tokenizeGameUiExpression(expression: string): Token[] {
  const tokens: Token[] = [];
  let cursor = 0;

  while (cursor < expression.length) {
    const char = expression[cursor];

    if (/\s/.test(char)) {
      cursor += 1;
      continue;
    }

    const operatorMatch = expression.slice(cursor).match(/^(==|!=|>=|<=|&&|\|\||>|<)/);
    if (operatorMatch) {
      tokens.push({
        type: "operator",
        value: operatorMatch[1] as OperatorToken["value"],
      });
      cursor += operatorMatch[1].length;
      continue;
    }

    if (char === "(" || char === ")") {
      tokens.push({ type: "paren", value: char });
      cursor += 1;
      continue;
    }

    if (char === "\"" || char === "'") {
      const { value, nextIndex } = readQuotedString(expression, cursor, char);
      tokens.push({ type: "string", value });
      cursor = nextIndex;
      continue;
    }

    const numberMatch = expression.slice(cursor).match(/^-?\d+(?:\.\d+)?/);
    if (numberMatch) {
      tokens.push({ type: "number", value: Number(numberMatch[0]) });
      cursor += numberMatch[0].length;
      continue;
    }

    const identifierMatch = expression.slice(cursor).match(/^[A-Za-z_][A-Za-z0-9_.-]*/);
    if (identifierMatch) {
      const identifier = identifierMatch[0];
      if (identifier === "true" || identifier === "false") {
        tokens.push({ type: "boolean", value: identifier === "true" });
      } else if (identifier === "null") {
        tokens.push({ type: "null", value: null });
      } else {
        tokens.push({ type: "identifier", value: identifier });
      }
      cursor += identifier.length;
      continue;
    }

    throw new Error(`Unsupported token in expression near "${expression.slice(cursor)}".`);
  }

  return tokens;
}

function readQuotedString(source: string, start: number, quote: string) {
  let cursor = start + 1;
  let value = "";

  while (cursor < source.length) {
    const char = source[cursor];
    if (char === "\\") {
      const next = source[cursor + 1];
      value += next ?? "";
      cursor += 2;
      continue;
    }
    if (char === quote) {
      return {
        value,
        nextIndex: cursor + 1,
      };
    }
    value += char;
    cursor += 1;
  }

  throw new Error("Unterminated string literal in expression.");
}

function resolveExpressionPath(scope: GameUiExpressionScope, path: string): unknown {
  if (!path || path.includes("[") || path.includes("]")) {
    return undefined;
  }

  const segments = path.split(".").map((segment) => segment.trim()).filter(Boolean);
  let current: unknown = scope;

  for (const segment of segments) {
    if (Array.isArray(current)) {
      if (segment === "length") {
        current = current.length;
        continue;
      }
      return undefined;
    }

    if (!current || typeof current !== "object") {
      return undefined;
    }

    current = (current as Record<string, unknown>)[segment];
  }

  return current;
}

function isComparisonToken(token: Token | undefined): token is Extract<Token, { type: "operator" }> & { value: ComparisonOperator } {
  return token?.type === "operator" && isComparisonOperator(token.value);
}

function isComparisonOperator(
  value: OperatorToken["value"],
): value is ComparisonOperator {
  return value === "=="
    || value === "!="
    || value === ">"
    || value === ">="
    || value === "<"
    || value === "<=";
}

function applyComparison(
  left: unknown,
  operator: ComparisonOperator,
  right: unknown,
): boolean {
  switch (operator) {
    case "==":
      return left === right;
    case "!=":
      return left !== right;
    case ">":
      return compareValues(left, right) > 0;
    case ">=":
      return compareValues(left, right) >= 0;
    case "<":
      return compareValues(left, right) < 0;
    case "<=":
      return compareValues(left, right) <= 0;
  }
}

function compareValues(left: unknown, right: unknown): number {
  if (typeof left === "number" && typeof right === "number") {
    return left - right;
  }

  const leftString = String(left ?? "");
  const rightString = String(right ?? "");
  if (leftString === rightString) {
    return 0;
  }
  return leftString > rightString ? 1 : -1;
}
