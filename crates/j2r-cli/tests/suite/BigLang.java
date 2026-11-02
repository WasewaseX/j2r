import java.util.ArrayList;
import java.util.HashMap;

/**
 * BigLang — real-program-scale differential fixture: lexer, recursive-
 * descent parser and evaluator for a tiny expression language with
 * variables, closures and control flow.
 *
 * Grammar:
 *   program := stmt*
 *   stmt    := "var" IDENT "=" expr ";"
 *            | "print" expr ";"
 *            | "fun" IDENT "(" params ")" block
 *            | "if" "(" expr ")" block ("else" block)?
 *            | "while" "(" expr ")" block
 *            | "return" expr ";"
 *   expr    := assignment (pratt: or -> and -> equality -> term -> factor
 *                         -> unary -> call -> primary)
 *   primary := NUMBER | STRING | IDENT | "(" expr ")" | "true" | "false"
 *            | "fn" "(" params ")" block   (anonymous closure)
 *
 * The corpus is a fixed set of programs; transcripts (including syntax
 * errors with positions and runtime errors) must match Temurin exactly.
 */
public class BigLang {

    // ------------------------------------------------------------------
    // Tokens
    // ------------------------------------------------------------------

    static final int T_INT = 0;
    static final int T_STR = 1;
    static final int T_IDENT = 2;
    static final int T_OP = 3;
    static final int T_KW = 4;
    static final int T_EOF = 5;

    static final class Tok {
        final int kind;
        final String text;
        final int pos;

        Tok(int kind, String text, int pos) {
            this.kind = kind;
            this.text = text;
            this.pos = pos;
        }

        public String toString() {
            return kind == T_INT ? "int(" + text + ")"
                : kind == T_STR ? "str(" + text + ")"
                : kind == T_IDENT ? "id(" + text + ")"
                : kind == T_KW ? "kw(" + text + ")"
                : kind == T_EOF ? "eof"
                : "op(" + text + ")";
        }
    }

    static final String[] KEYWORDS = {
        "var", "print", "fun", "if", "else", "while", "return", "true", "false", "fn",
    };

    static boolean isKeyword(String s) {
        for (int i = 0; i < KEYWORDS.length; i++) {
            if (KEYWORDS[i].equals(s)) {
                return true;
            }
        }
        return false;
    }

    static boolean isIdentStart(char c) {
        return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_';
    }

    static boolean isIdentPart(char c) {
        return isIdentStart(c) || (c >= '0' && c <= '9');
    }

    static class LexError extends RuntimeException {
        LexError(String msg, int pos) {
            super(msg + " at " + pos);
        }
    }

    static ArrayList<Tok> lex(String src) {
        ArrayList<Tok> out = new ArrayList<Tok>();
        int i = 0;
        while (i < src.length()) {
            char c = src.charAt(i);
            if (c == ' ' || c == '\n' || c == '\t' || c == '\r') {
                i++;
            } else if (c >= '0' && c <= '9') {
                int start = i;
                while (i < src.length() && src.charAt(i) >= '0' && src.charAt(i) <= '9') {
                    i++;
                }
                out.add(new Tok(T_INT, src.substring(start, i), start));
            } else if (isIdentStart(c)) {
                int start = i;
                while (i < src.length() && isIdentPart(src.charAt(i))) {
                    i++;
                }
                String word = src.substring(start, i);
                out.add(new Tok(isKeyword(word) ? T_KW : T_IDENT, word, start));
            } else if (c == '"') {
                int start = i;
                i++;
                StringBuilder sb = new StringBuilder();
                while (i < src.length() && src.charAt(i) != '"') {
                    sb.append(src.charAt(i));
                    i++;
                }
                if (i >= src.length()) {
                    throw new LexError("unterminated string", start);
                }
                i++;
                out.add(new Tok(T_STR, sb.toString(), start));
            } else {
                // Two-char operators first.
                if (i + 1 < src.length()) {
                    String two = src.substring(i, i + 2);
                    if (two.equals("==") || two.equals("!=") || two.equals("<=")
                        || two.equals(">=") || two.equals("&&") || two.equals("||")) {
                        out.add(new Tok(T_OP, two, i));
                        i += 2;
                        continue;
                    }
                }
                if ("+-*/%()<>=;{},".indexOf(c) >= 0) {
                    out.add(new Tok(T_OP, String.valueOf(c), i));
                    i++;
                } else {
                    throw new LexError("unexpected character '" + c + "'", i);
                }
            }
        }
        out.add(new Tok(T_EOF, "", src.length()));
        return out;
    }

    // ------------------------------------------------------------------
    // AST
    // ------------------------------------------------------------------

    abstract static class Expr {
        final int pos;
        Expr(int pos) {
            this.pos = pos;
        }
    }

    static final class Num extends Expr {
        final long value;
        Num(long value, int pos) {
            super(pos);
            this.value = value;
        }
    }

    static final class Str extends Expr {
        final String value;
        Str(String value, int pos) {
            super(pos);
            this.value = value;
        }
    }

    static final class Var extends Expr {
        final String name;
        Var(String name, int pos) {
            super(pos);
            this.name = name;
        }
    }

    static final class Bool extends Expr {
        final boolean value;
        Bool(boolean value, int pos) {
            super(pos);
            this.value = value;
        }
    }

    static final class BinOp extends Expr {
        final String op;
        final Expr left;
        final Expr right;
        BinOp(String op, Expr left, Expr right, int pos) {
            super(pos);
            this.op = op;
            this.left = left;
            this.right = right;
        }
    }

    static final class Unary extends Expr {
        final String op;
        final Expr operand;
        Unary(String op, Expr operand, int pos) {
            super(pos);
            this.op = op;
            this.operand = operand;
        }
    }

    static final class Call extends Expr {
        final Expr callee;
        final ArrayList<Expr> args;
        Call(Expr callee, ArrayList<Expr> args, int pos) {
            super(pos);
            this.callee = callee;
            this.args = args;
        }
    }

    abstract static class Stmt {
        final int pos;
        Stmt(int pos) {
            this.pos = pos;
        }
    }

    static final class VarDecl extends Stmt {
        final String name;
        final Expr init;
        VarDecl(String name, Expr init, int pos) {
            super(pos);
            this.name = name;
            this.init = init;
        }
    }

    static final class Print extends Stmt {
        final Expr value;
        Print(Expr value, int pos) {
            super(pos);
            this.value = value;
        }
    }

    static final class FunDecl extends Stmt {
        final String name;
        final ArrayList<String> params;
        final ArrayList<Stmt> body;
        FunDecl(String name, ArrayList<String> params, ArrayList<Stmt> body, int pos) {
            super(pos);
            this.name = name;
            this.params = params;
            this.body = body;
        }
    }

    static final class If extends Stmt {
        final Expr cond;
        final ArrayList<Stmt> thenB;
        final ArrayList<Stmt> elseB; // null when absent
        If(Expr cond, ArrayList<Stmt> thenB, ArrayList<Stmt> elseB, int pos) {
            super(pos);
            this.cond = cond;
            this.thenB = thenB;
            this.elseB = elseB;
        }
    }

    static final class While extends Stmt {
        final Expr cond;
        final ArrayList<Stmt> body;
        While(Expr cond, ArrayList<Stmt> body, int pos) {
            super(pos);
            this.cond = cond;
            this.body = body;
        }
    }

    static final class Return extends Stmt {
        final Expr value;
        Return(Expr value, int pos) {
            super(pos);
            this.value = value;
        }
    }

    static final class Assign extends Stmt {
        final String name;
        final Expr value;
        Assign(String name, Expr value, int pos) {
            super(pos);
            this.name = name;
            this.value = value;
        }
    }

    static final class ExprStmt extends Stmt {
        final Expr expr;
        ExprStmt(Expr expr, int pos) {
            super(pos);
            this.expr = expr;
        }
    }

    /** Non-local control flow for `return`. */
    static class ReturnSignal extends RuntimeException {
        final Value value;
        ReturnSignal(Value value) {
            super("return");
            this.value = value;
        }
    }

    static class ParseError extends RuntimeException {
        ParseError(String msg, int pos) {
            super(msg + " at " + pos);
        }
    }

    // ------------------------------------------------------------------
    // Parser (recursive descent)
    // ------------------------------------------------------------------

    static final class Parser {
        final ArrayList<Tok> toks;
        int i;

        Parser(ArrayList<Tok> toks) {
            this.toks = toks;
        }

        Tok peek() {
            return toks.get(i);
        }

        Tok advance() {
            Tok t = toks.get(i);
            if (t.kind != T_EOF) {
                i++;
            }
            return t;
        }

        boolean matchOp(String op) {
            Tok t = peek();
            if (t.kind == T_OP && t.text.equals(op)) {
                i++;
                return true;
            }
            return false;
        }

        boolean matchKw(String kw) {
            Tok t = peek();
            if (t.kind == T_KW && t.text.equals(kw)) {
                i++;
                return true;
            }
            return false;
        }

        Tok expectOp(String op) {
            Tok t = peek();
            if (t.kind != T_OP || !t.text.equals(op)) {
                throw new ParseError("expected '" + op + "' but got " + t, t.pos);
            }
            return advance();
        }

        Tok expectKw(String kw) {
            Tok t = peek();
            if (t.kind != T_KW || !t.text.equals(kw)) {
                throw new ParseError("expected '" + kw + "' but got " + t, t.pos);
            }
            return advance();
        }

        Tok expectIdent() {
            Tok t = peek();
            if (t.kind != T_IDENT) {
                throw new ParseError("expected identifier but got " + t, t.pos);
            }
            return advance();
        }

        ArrayList<Stmt> parseProgram() {
            ArrayList<Stmt> stmts = new ArrayList<Stmt>();
            while (peek().kind != T_EOF) {
                stmts.add(parseStmt());
            }
            return stmts;
        }

        ArrayList<Stmt> parseBlock() {
            expectOp("{");
            ArrayList<Stmt> stmts = new ArrayList<Stmt>();
            while (!matchOp("}")) {
                if (peek().kind == T_EOF) {
                    throw new ParseError("unterminated block", peek().pos);
                }
                stmts.add(parseStmt());
            }
            return stmts;
        }

        Stmt parseStmt() {
            Tok t = peek();
            if (t.kind == T_KW) {
                switch (t.text) {
                    case "var": {
                        advance();
                        Tok name = expectIdent();
                        expectOp("=");
                        Expr init = parseExpr();
                        expectOp(";");
                        return new VarDecl(name.text, init, t.pos);
                    }
                    case "print": {
                        advance();
                        Expr v = parseExpr();
                        expectOp(";");
                        return new Print(v, t.pos);
                    }
                    case "fun": {
                        advance();
                        Tok name = expectIdent();
                        expectOp("(");
                        ArrayList<String> params = parseParams();
                        expectOp(")");
                        ArrayList<Stmt> body = parseBlock();
                        return new FunDecl(name.text, params, body, t.pos);
                    }
                    case "if": {
                        advance();
                        expectOp("(");
                        Expr cond = parseExpr();
                        expectOp(")");
                        ArrayList<Stmt> thenB = parseBlock();
                        ArrayList<Stmt> elseB = null;
                        if (matchKw("else")) {
                            elseB = parseBlock();
                        }
                        return new If(cond, thenB, elseB, t.pos);
                    }
                    case "while": {
                        advance();
                        expectOp("(");
                        Expr cond = parseExpr();
                        expectOp(")");
                        ArrayList<Stmt> body = parseBlock();
                        return new While(cond, body, t.pos);
                    }
                    case "return": {
                        advance();
                        Expr v = parseExpr();
                        expectOp(";");
                        return new Return(v, t.pos);
                    }
                    default:
                        throw new ParseError("unexpected keyword '" + t.text + "'", t.pos);
                }
            }
            if (t.kind == T_IDENT && i + 1 < toks.size()
                && toks.get(i + 1).kind == T_OP && toks.get(i + 1).text.equals("=")) {
                Tok name = advance();
                advance(); // '='
                Expr v = parseExpr();
                expectOp(";");
                return new Assign(name.text, v, t.pos);
            }
            Expr e = parseExpr();
            expectOp(";");
            return new ExprStmt(e, t.pos);
        }

        ArrayList<String> parseParams() {
            ArrayList<String> params = new ArrayList<String>();
            if (peek().kind == T_IDENT) {
                params.add(advance().text);
                while (matchOp(",")) {
                    params.add(expectIdent().text);
                }
            }
            return params;
        }

        Expr parseExpr() {
            return parseOr();
        }

        Expr parseOr() {
            Expr left = parseAnd();
            while (matchOp("||")) {
                Tok op = toks.get(i - 1);
                left = new BinOp("||", left, parseAnd(), op.pos);
            }
            return left;
        }

        Expr parseAnd() {
            Expr left = parseEquality();
            while (matchOp("&&")) {
                Tok op = toks.get(i - 1);
                left = new BinOp("&&", left, parseEquality(), op.pos);
            }
            return left;
        }

        Expr parseEquality() {
            Expr left = parseComparison();
            while (peek().kind == T_OP && (peek().text.equals("==") || peek().text.equals("!="))) {
                Tok op = advance();
                left = new BinOp(op.text, left, parseComparison(), op.pos);
            }
            return left;
        }

        Expr parseComparison() {
            Expr left = parseTerm();
            while (peek().kind == T_OP && (peek().text.equals("<") || peek().text.equals(">")
                || peek().text.equals("<=") || peek().text.equals(">="))) {
                Tok op = advance();
                left = new BinOp(op.text, left, parseTerm(), op.pos);
            }
            return left;
        }

        Expr parseTerm() {
            Expr left = parseFactor();
            while (peek().kind == T_OP && (peek().text.equals("+") || peek().text.equals("-"))) {
                Tok op = advance();
                left = new BinOp(op.text, left, parseFactor(), op.pos);
            }
            return left;
        }

        Expr parseFactor() {
            Expr left = parseUnary();
            while (peek().kind == T_OP && (peek().text.equals("*") || peek().text.equals("/")
                || peek().text.equals("%"))) {
                Tok op = advance();
                left = new BinOp(op.text, left, parseUnary(), op.pos);
            }
            return left;
        }

        Expr parseUnary() {
            if (peek().kind == T_OP && (peek().text.equals("-") || peek().text.equals("!"))) {
                Tok op = advance();
                return new Unary(op.text, parseUnary(), op.pos);
            }
            return parseCall();
        }

        Expr parseCall() {
            Expr callee = parsePrimary();
            while (matchOp("(")) {
                ArrayList<Expr> args = new ArrayList<Expr>();
                if (!matchOp(")")) {
                    args.add(parseExpr());
                    while (matchOp(",")) {
                        args.add(parseExpr());
                    }
                    expectOp(")");
                }
                callee = new Call(callee, args, callee.pos);
            }
            return callee;
        }

        Expr parsePrimary() {
            Tok t = peek();
            if (t.kind == T_INT) {
                advance();
                return new Num(Long.parseLong(t.text), t.pos);
            }
            if (t.kind == T_STR) {
                advance();
                return new Str(t.text, t.pos);
            }
            if (t.kind == T_IDENT) {
                advance();
                return new Var(t.text, t.pos);
            }
            if (t.kind == T_KW && t.text.equals("true")) {
                advance();
                return new Bool(true, t.pos);
            }
            if (t.kind == T_KW && t.text.equals("false")) {
                advance();
                return new Bool(false, t.pos);
            }
            if (t.kind == T_KW && t.text.equals("fn")) {
                advance();
                expectOp("(");
                ArrayList<String> params = parseParams();
                expectOp(")");
                ArrayList<Stmt> body = parseBlock();
                return new ClosureExpr(params, body, t.pos);
            }
            if (t.kind == T_OP && t.text.equals("(")) {
                advance();
                Expr e = parseExpr();
                expectOp(")");
                return e;
            }
            throw new ParseError("unexpected token " + t, t.pos);
        }
    }

    static final class ClosureExpr extends Expr {
        final ArrayList<String> params;
        final ArrayList<Stmt> body;
        ClosureExpr(ArrayList<String> params, ArrayList<Stmt> body, int pos) {
            super(pos);
            this.params = params;
            this.body = body;
        }
    }

    // ------------------------------------------------------------------
    // Values + environment
    // ------------------------------------------------------------------

    /** Runtime value: long, string, boolean or closure. */
    abstract static class Value {
    }

    static final class VLong extends Value {
        final long v;
        VLong(long v) {
            this.v = v;
        }
    }

    static final class VStr extends Value {
        final String v;
        VStr(String v) {
            this.v = v;
        }
    }

    static final class VBool extends Value {
        final boolean v;
        VBool(boolean v) {
            this.v = v;
        }
    }

    static final class VClosure extends Value {
        final ArrayList<String> params;
        final ArrayList<Stmt> body;
        final Env capture;
        VClosure(ArrayList<String> params, ArrayList<Stmt> body, Env capture) {
            this.params = params;
            this.body = body;
            this.capture = capture;
        }
    }

    static final class Env {
        final HashMap<String, Value> vars = new HashMap<String, Value>();
        final Env parent;

        Env(Env parent) {
            this.parent = parent;
        }

        Value get(String name, int pos) {
            Env e = this;
            while (e != null) {
                if (e.vars.containsKey(name)) {
                    return e.vars.get(name);
                }
                e = e.parent;
            }
            throw new RuntimeException("undefined variable '" + name + "' at " + pos);
        }

        void define(String name, Value v) {
            vars.put(name, v);
        }

        boolean has(String name) {
            return vars.containsKey(name);
        }

        void set(String name, Value v, int pos) {
            Env e = this;
            while (e != null) {
                if (e.vars.containsKey(name)) {
                    e.vars.put(name, v);
                    return;
                }
                e = e.parent;
            }
            throw new RuntimeException("undefined variable '" + name + "' at " + pos);
        }
    }

    // ------------------------------------------------------------------
    // Evaluator
    // ------------------------------------------------------------------

    static Value eval(Expr e, Env env) {
        if (e instanceof Num) {
            return new VLong(((Num) e).value);
        }
        if (e instanceof Str) {
            return new VStr(((Str) e).value);
        }
        if (e instanceof Bool) {
            return new VBool(((Bool) e).value);
        }
        if (e instanceof ClosureExpr) {
            ClosureExpr ce = (ClosureExpr) e;
            return new VClosure(ce.params, ce.body, env);
        }
        if (e instanceof Var) {
            Var v = (Var) e;
            return env.get(v.name, v.pos);
        }
        if (e instanceof Unary) {
            Unary u = (Unary) e;
            Value v = eval(u.operand, env);
            if (u.op.equals("-")) {
                return new VLong(-asLong(v, u.pos));
            }
            return new VBool(!asBool(v, u.pos));
        }
        if (e instanceof BinOp) {
            BinOp b = (BinOp) e;
            // Short-circuit forms first.
            if (b.op.equals("&&")) {
                if (!asBool(eval(b.left, env), b.pos)) {
                    return new VBool(false);
                }
                return new VBool(asBool(eval(b.right, env), b.pos));
            }
            if (b.op.equals("||")) {
                if (asBool(eval(b.left, env), b.pos)) {
                    return new VBool(true);
                }
                return new VBool(asBool(eval(b.right, env), b.pos));
            }
            Value l = eval(b.left, env);
            Value r = eval(b.right, env);
            if (b.op.equals("+") && l instanceof VStr) {
                return new VStr(((VStr) l).v + stringify(r));
            }
            // Equality spans all value kinds (strings, booleans, numbers;
            // closures compare by identity) — before numeric coercion.
            if (b.op.equals("==")) {
                return new VBool(valueEquals(l, r));
            }
            if (b.op.equals("!=")) {
                return new VBool(!valueEquals(l, r));
            }
            long a1 = asLong(l, b.pos);
            long a2 = asLong(r, b.pos);
            switch (b.op) {
                case "+": return new VLong(a1 + a2);
                case "-": return new VLong(a1 - a2);
                case "*": return new VLong(a1 * a2);
                case "/":
                    if (a2 == 0) {
                        throw new RuntimeException("division by zero at " + b.pos);
                    }
                    return new VLong(a1 / a2);
                case "%":
                    if (a2 == 0) {
                        throw new RuntimeException("modulo by zero at " + b.pos);
                    }
                    return new VLong(a1 % a2);
                case "==": return new VBool(a1 == a2);
    
                case "<": return new VBool(a1 < a2);
                case ">": return new VBool(a1 > a2);
                case "<=": return new VBool(a1 <= a2);
                case ">=": return new VBool(a1 >= a2);
                default:
                    throw new RuntimeException("unknown operator " + b.op);
            }
        }
        if (e instanceof Call) {
            Call c = (Call) e;
            Value fn = eval(c.callee, env);
            if (!(fn instanceof VClosure)) {
                throw new RuntimeException("calling non-function at " + c.pos);
            }
            VClosure cl = (VClosure) fn;
            if (c.args.size() != cl.params.size()) {
                throw new RuntimeException("arity mismatch at " + c.pos
                    + ": want " + cl.params.size() + " got " + c.args.size());
            }
            Env call = new Env(cl.capture);
            for (int i = 0; i < cl.params.size(); i++) {
                call.define(cl.params.get(i), eval(c.args.get(i), env));
            }
            try {
                execBlock(cl.body, call);
            } catch (ReturnSignal rs) {
                return rs.value;
            }
            return new VLong(0);
        }
        throw new RuntimeException("cannot evaluate " + e.getClass().getName());
    }

    static void execBlock(ArrayList<Stmt> stmts, Env env) {
        for (int i = 0; i < stmts.size(); i++) {
            exec(stmts.get(i), env);
        }
    }

    static void exec(Stmt s, Env env) {
        if (s instanceof VarDecl) {
            VarDecl vd = (VarDecl) s;
            env.define(vd.name, eval(vd.init, env));
            return;
        }
        if (s instanceof Print) {
            System.out.println(stringify(eval(((Print) s).value, env)));
            return;
        }
        if (s instanceof FunDecl) {
            FunDecl fd = (FunDecl) s;
            env.define(fd.name, new VClosure(fd.params, fd.body, env));
            return;
        }
        if (s instanceof If) {
            If st = (If) s;
            if (asBool(eval(st.cond, env), st.pos)) {
                execBlock(st.thenB, env);
            } else if (st.elseB != null) {
                execBlock(st.elseB, env);
            }
            return;
        }
        if (s instanceof While) {
            While w = (While) s;
            while (asBool(eval(w.cond, env), w.pos)) {
                execBlock(w.body, env);
            }
            return;
        }
        if (s instanceof Return) {
            throw new ReturnSignal(eval(((Return) s).value, env));
        }
        if (s instanceof Assign) {
            Assign as = (Assign) s;
            env.set(as.name, eval(as.value, env), as.pos);
            return;
        }
        if (s instanceof ExprStmt) {
            eval(((ExprStmt) s).expr, env);
            return;
        }
        throw new RuntimeException("cannot execute " + s.getClass().getName());
    }

    static boolean valueEquals(Value a, Value b) {
        if (a instanceof VLong && b instanceof VLong) {
            return ((VLong) a).v == ((VLong) b).v;
        }
        if (a instanceof VStr && b instanceof VStr) {
            return ((VStr) a).v.equals(((VStr) b).v);
        }
        if (a instanceof VBool && b instanceof VBool) {
            return ((VBool) a).v == ((VBool) b).v;
        }
        return a == b;
    }

    static long asLong(Value v, int pos) {
        if (v instanceof VLong) {
            return ((VLong) v).v;
        }
        throw new RuntimeException("expected number at " + pos);
    }

    static boolean asBool(Value v, int pos) {
        if (v instanceof VBool) {
            return ((VBool) v).v;
        }
        throw new RuntimeException("expected boolean at " + pos);
    }

    static String stringify(Value v) {
        if (v instanceof VLong) {
            return Long.toString(((VLong) v).v);
        }
        if (v instanceof VStr) {
            return ((VStr) v).v;
        }
        if (v instanceof VBool) {
            return ((VBool) v).v ? "true" : "false";
        }
        return "<fn>";
    }

    static String run(String src) {
        ArrayList<Tok> toks = lex(src);
        Parser p = new Parser(toks);
        ArrayList<Stmt> prog = p.parseProgram();
        Env global = new Env(null);
        try {
            execBlock(prog, global);
        } catch (ReturnSignal rs) {
            // Top-level return just ends the program.
        }
        return "ok";
    }

    static void expectFail(String src, String wantPrefix) {
        try {
            run(src);
            throw new IllegalStateException("expected failure for program: " + src);
        } catch (ParseError | LexError e) {
            String msg = e.getMessage();
            if (!msg.startsWith(wantPrefix)) {
                throw new IllegalStateException(
                    "wrong error for [" + src + "]: " + msg);
            }
            System.out.println("err  " + msg);
        }
    }

    // ------------------------------------------------------------------
    // Corpus
    // ------------------------------------------------------------------

    public static void main(String[] args) {
        // 1. Arithmetic and precedence.
        run("print 1 + 2 * 3; print (1 + 2) * 3; print 10 % 4; print -7 / 2;");
        // 2. Strings and concatenation with +.
        run("print \"abc\"; print \"n=\" + (5 * 6);");
        // 3. Variables and assignment.
        run("var x = 10; var y = x * 2; print x + y;");
        // 4. Control flow: fib via while.
        run("var a = 0; var b = 1; var n = 0; while (n < 20) { var t = a + b; a = b; b = t; n = n + 1; } print a;");
        // 5. Functions, recursion, return.
        run("fun fact(n) { if (n <= 1) { return 1; } return n * fact(n - 1); } print fact(10);");
        // 6. Closures: counter factory.
        run("fun makeCounter(start) { var c = start; return fn() { c = c + 1; return c; }; } "
            + "var c1 = makeCounter(10); var c2 = makeCounter(100); "
            + "print c1(); print c1(); print c2(); print c1();");
        // 7. Higher-order functions: map/filter over an accumulator.
        run("fun twice(f, x) { return f(f(x)); } var inc = fn(x) { return x + 1; }; print twice(inc, 40);");
        // 8. Short-circuit evaluation (division-by-zero must NOT run).
        run("var z = 0; if (false && 1 / z > 0) { print \"bad\"; } else { print \"short-circuit\"; }");
        // 9. While with early return.
        run("fun firstSqrt(n) { var i = 0; while (true) { if (i * i >= n) { return i; } i = i + 1; } } "
            + "print firstSqrt(50);");
        // 10. String comparisons via ==.
        run("var s = \"lang\"; if (s == \"lang\") { print \"same\"; } else { print \"diff\"; }");

        expectFail("print 1 + ;", "unexpected token op(;)");
        expectFail("var = 5;", "expected identifier but got op(=)");
        expectFail("print (1 + 2;", "expected ')' but got op(;)");
        expectFail("fun f( { }", "expected ')' but got op({)");
        expectFail("print \"unterminated", "unterminated string at 6");
        expectFail("print 1 @ 2;", "unexpected character '@' at 8");
        expectFail("else { }", "unexpected keyword 'else'");

        // Runtime errors are RuntimeExceptions with deterministic text.
        try {
            run("print nope;");
            System.out.println("unreachable");
        } catch (RuntimeException e) {
            System.out.println("caught: " + e.getMessage());
        }
        try {
            run("print 1 / 0;");
            System.out.println("unreachable");
        } catch (RuntimeException e) {
            System.out.println("caught: " + e.getMessage());
        }
        try {
            run("var f = 3; print f();");
            System.out.println("unreachable");
        } catch (RuntimeException e) {
            System.out.println("caught: " + e.getMessage());
        }

        System.out.println("BigLang done");
    }
}
