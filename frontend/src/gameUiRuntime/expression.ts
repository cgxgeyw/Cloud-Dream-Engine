type GameUiExpressionScope = Record<string, unknown>;

type Token =
  | { type: "identifier"; value: string }
  | { type: "string"; value: string }
  | { type: "number"; value: number }
  | { type: "boolean"; value: boolean }
  | { type: "null"; value: null }
  | { type: "operator"; value: "==" | "!=" | ">" | ">=" | "<" | "<=" | "&&" | "||" | "+" | "-" | "*" | "/" | "%" }
  | { type: "paren"; value: "(" | ")" }
  | { type: "comma"; value: "," };

type OperatorToken = Extract<Token, { type: "operator" }>;
type ComparisonOperator = "==" | "!=" | ">" | ">=" | "<" | "<=";
type ArithmeticOperator = "+" | "-" | "*" | "/" | "%";

/**
 * 可在表达式里调用的函数。刻意做成**闭集白名单**而不是任意求值：
 * 世界包是外部输入，`eval` 或任意函数调用会把它变成逃逸口。
 * 新增函数必须同时登记到后端 `validate_expression` 的同名清单。
 *
 * 统一签名收成 (...args: number[]) 让分派处不必为不同形参数量写重载；
 * 各函数内部自己做参数个数校验。
 */
type GameUiExpressionFunction = (...args: number[]) => unknown;

const GAME_UI_EXPRESSION_FUNCTIONS: Record<string, GameUiExpressionFunction> = {
  math_min: (...args) => Math.min(...args),
  math_max: (...args) => Math.max(...args),
  math_abs: (value) => Math.abs(value),
  math_floor: (value) => Math.floor(value),
  math_ceil: (value) => Math.ceil(value),
  math_round: (value) => Math.round(value),
  /** 0~1 随机小数。每次求值重新掷。 */
  math_random: () => Math.random(),
  /** [min, max] 闭区间随机整数；只传一个参数时返回 0~该值的整数。 */
  math_random_int: (min, max) => {
    const low = max === undefined ? 0 : Math.ceil(Number(min));
    const high = max === undefined ? Math.floor(Number(min)) : Math.floor(Number(max));
    if (!Number.isFinite(low) || !Number.isFinite(high) || high < low) {
      return low;
    }
    return low + Math.floor(Math.random() * (high - low + 1));
  },
  math_clamp: (value, min, max) => Math.min(Math.max(value, min), max),
  /** 线性插值，t 取 0~1；超出范围自动夹紧。 */
  math_lerp: (from, to, t) => from + (to - from) * Math.min(Math.max(t, 0), 1),
};

const GAME_UI_EXPRESSION_FUNCTION_NAMES = new Set<string>(
  Object.keys(GAME_UI_EXPRESSION_FUNCTIONS),
);

/** 供文档与后端核对：可调用的函数名清单。 */
export const GAME_UI_EXPRESSION_FUNCTION_IDS: readonly string[] = [
  ...GAME_UI_EXPRESSION_FUNCTION_NAMES,
];

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
      throw new Error("表达式意外结束。");
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
    let left = parseAdditive();

    while (isComparisonToken(peek())) {
      const operator = (consume() as OperatorToken).value as ComparisonOperator;
      const right = parseAdditive();
      left = applyComparison(left, operator, right);
    }

    return left;
  }

  /** 加减：优先级低于比较，高于乘除模。 */
  function parseAdditive(): unknown {
    let left = parseMultiplicative();
    while (isArithmeticToken(peek()) && (peek() as OperatorToken).value !== "*" && (peek() as OperatorToken).value !== "/" && (peek() as OperatorToken).value !== "%") {
      const operator = (consume() as OperatorToken).value as ArithmeticOperator;
      const right = parseMultiplicative();
      left = applyArithmetic(left, operator, right);
    }
    return left;
  }

  function parseMultiplicative(): unknown {
    let left = parseUnary();
    while (isArithmeticToken(peek()) && (peek() as OperatorToken).value !== "+" && (peek() as OperatorToken).value !== "-") {
      const operator = (consume() as OperatorToken).value as ArithmeticOperator;
      const right = parseUnary();
      left = applyArithmetic(left, operator, right);
    }
    return left;
  }

  /** 一元负号，让 `-1` 与 `hp - 1` 里的负号都能解析。 */
  function parseUnary(): unknown {
    if (peek()?.type === "operator" && (peek() as OperatorToken).value === "-") {
      consume();
      return applyArithmetic(0, "-", parseUnary());
    }
    return parsePrimary();
  }

  function parsePrimary(): unknown {
    const token = consume();

    if (token.type === "paren" && token.value === "(") {
      const value = parseExpression();
      const next = consume();
      if (next.type !== "paren" || next.value !== ")") {
        throw new Error("表达式缺少右括号。");
      }
      return value;
    }

    if (token.type === "identifier") {
      // 函数调用：标识符后紧跟左括号。只认白名单里的名字。
      if (peek()?.type === "paren" && peek()?.value === "(") {
        return parseFunctionCall(token.value);
      }
      return resolveExpressionPath(scope, token.value);
    }

    if (token.type === "string" || token.type === "number" || token.type === "boolean") {
      return token.value;
    }

    if (token.type === "null") {
      return null;
    }

    throw new Error("表达式中出现意外标记。");
  }

  function parseFunctionCall(name: string): unknown {
    if (!GAME_UI_EXPRESSION_FUNCTION_NAMES.has(name)) {
      throw new Error(`表达式不支持调用 ${name}()。`);
    }
    consume(); // 吃掉左括号

    const args: unknown[] = [];
    if (peek()?.type === "paren" && peek()?.value === ")") {
      consume();
    } else {
      for (;;) {
        args.push(parseExpression());
        const next = peek();
        if (next?.type === "comma") {
          consume();
          continue;
        }
        if (next?.type === "paren" && next.value === ")") {
          consume();
          break;
        }
        throw new Error("表达式函数调用的参数列表不完整。");
      }
    }

    const fn = GAME_UI_EXPRESSION_FUNCTIONS[name];
    return fn(...(args as number[]));
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

    const operatorMatch = expression.slice(cursor).match(/^(==|!=|>=|<=|&&|\|\||[+\-*/%]|>|<)/);
    if (operatorMatch) {
      tokens.push({
        type: "operator",
        value: operatorMatch[1] as OperatorToken["value"],
      });
      cursor += operatorMatch[1].length;
      continue;
    }

    if (char === ",") {
      tokens.push({ type: "comma", value: "," });
      cursor += 1;
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

  throw new Error("表达式中字符串字面量未闭合。");
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

function isArithmeticToken(token: Token | undefined): token is Extract<Token, { type: "operator" }> & { value: ArithmeticOperator } {
  if (token?.type !== "operator") {
    return false;
  }
  const value = token.value;
  return value === "+" || value === "-" || value === "*" || value === "/" || value === "%";
}

function toArithmeticNumber(value: unknown): number {
  if (typeof value === "number") {
    return value;
  }
  if (typeof value === "boolean") {
    return value ? 1 : 0;
  }
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : Number.NaN;
}

/**
 * 算术求值。`+` 遇到字符串走拼接（与 JS 一致，方便拼展示文案），
 * 其余运算符把操作数转成数字；转换失败得到 NaN，交给上层比较自然为 false。
 */
function applyArithmetic(left: unknown, operator: ArithmeticOperator, right: unknown): unknown {
  if (operator === "+" && (typeof left === "string" || typeof right === "string")) {
    return `${left ?? ""}${right ?? ""}`;
  }

  const a = toArithmeticNumber(left);
  const b = toArithmeticNumber(right);

  switch (operator) {
    case "+":
      return a + b;
    case "-":
      return a - b;
    case "*":
      return a * b;
    case "/":
      // 除零得 Infinity 而非抛错：条件里出现它只会让比较为真/为假，不该炸掉整页。
      return a / b;
    case "%":
      return b === 0 ? Number.NaN : a % b;
  }
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
