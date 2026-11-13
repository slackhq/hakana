enum Suit: string as string {
    HEARTS = 'hearts';
    SPADES = 'spades';
}

enum Opaque: int {
    ONE = 1;
}

interface Animal {}
abstract class Dog implements Animal {}
final class Puppy extends Dog {}
final class Cat implements Animal {}

type Point = shape('x' => int, 'y' => int);

function fromInt(int $i): int {
    return HH\FIXME\UNSAFE_CAST<mixed, int>($i);
}

function fromNonnullInt(int $i): int {
    return HH\FIXME\UNSAFE_CAST<?int, int>($i);
}

function fromArraykeyInt(int $i): string {
    return HH\FIXME\UNSAFE_CAST<arraykey, string>($i);
}

function afterNarrowing(mixed $m): int {
    if ($m is int) {
        return HH\FIXME\UNSAFE_CAST<mixed, int>($m);
    }
    return 0;
}

function afterNullCheck(?string $s): string {
    if ($s is null) {
        return '';
    }
    return HH\FIXME\UNSAFE_CAST<?string, string>($s);
}

function fromLiteral(): int {
    return HH\FIXME\UNSAFE_CAST<mixed, int>('a');
}

function fromSubclass(Dog $d): Cat {
    return HH\FIXME\UNSAFE_CAST<Animal, Cat>($d);
}

function fromNested(vec<int> $v): vec<string> {
    return HH\FIXME\UNSAFE_CAST<vec<arraykey>, vec<string>>($v);
}

function fromNestedSubclass(dict<string, Puppy> $d): dict<string, Cat> {
    return HH\FIXME\UNSAFE_CAST<dict<arraykey, Dog>, dict<string, Cat>>($d);
}

function fromTransparentEnum(Suit $s): int {
    return HH\FIXME\UNSAFE_CAST<string, int>($s);
}

function fromBoundedTemplate<T as Dog>(T $t): Cat {
    return HH\FIXME\UNSAFE_CAST<mixed, Cat>($t);
}

function fromRequiredField(shape('a' => int, 'b' => string) $s): int {
    return HH\FIXME\UNSAFE_CAST<shape('a' => int, ?'b' => string), int>($s);
}

function piped(int $i): string {
    return $i |> HH\FIXME\UNSAFE_CAST<mixed, string>($$);
}

function inLoop(vec<int> $v): int {
    $total = 0;
    foreach ($v as $i) {
        $total = HH\FIXME\UNSAFE_CAST<mixed, int>($total) + $i;
    }
    return $total;
}

function suppressed(int $i): int {
    /* HAKANA_FIXME[InvalidArgument] kept as it is */
    return HH\FIXME\UNSAFE_CAST<mixed, int>($i);
}

// Nothing below is reported: the argument is as general as the type given
// for it, or cannot be shown to be less so.

function exact(int $i): string {
    return HH\FIXME\UNSAFE_CAST<int, string>($i);
}

function exactMixed(mixed $m): int {
    return HH\FIXME\UNSAFE_CAST<mixed, int>($m);
}

function exactNullable(?int $i): int {
    return HH\FIXME\UNSAFE_CAST<?int, int>($i);
}

function exactUnion(int $i, string $s, bool $b): int {
    $v = $b ? $i : $s;
    return HH\FIXME\UNSAFE_CAST<arraykey, int>($v);
}

function exactNested(dict<string, vec<Animal>> $d): dict<string, vec<Dog>> {
    return HH\FIXME\UNSAFE_CAST<dict<string, vec<Animal>>, dict<string, vec<Dog>>>($d);
}

function exactAlias(Point $p): int {
    return HH\FIXME\UNSAFE_CAST<Point, int>($p);
}

function exactShape(shape('x' => int, 'y' => int) $p): int {
    return HH\FIXME\UNSAFE_CAST<Point, int>($p);
}

function literalString(): int {
    return HH\FIXME\UNSAFE_CAST<string, int>('a');
}

function literalInt(): string {
    $i = 5;
    return HH\FIXME\UNSAFE_CAST<int, string>($i);
}

function literalBool(): string {
    return HH\FIXME\UNSAFE_CAST<bool, string>(true);
}

function interpolated(int $i): int {
    $s = "id-$i";
    return HH\FIXME\UNSAFE_CAST<string, int>($s);
}

function enumCase(): int {
    return HH\FIXME\UNSAFE_CAST<Suit, int>(Suit::HEARTS);
}

function opaqueEnum(Opaque $o): string {
    return HH\FIXME\UNSAFE_CAST<Opaque, string>($o);
}

function vecLiteral(): vec<string> {
    $v = vec[1, 2];
    return HH\FIXME\UNSAFE_CAST<vec<int>, vec<string>>($v);
}

function mixedVecLiteral(): vec<string> {
    $v = vec[1, 'a'];
    return HH\FIXME\UNSAFE_CAST<vec<arraykey>, vec<string>>($v);
}

function vectorOfLiterals(): Vector<int> {
    $v = Vector {'a', 'b'};
    return HH\FIXME\UNSAFE_CAST<Vector<string>, Vector<int>>($v);
}

async function awaitableOfLiteral(): Awaitable<int> {
    $a = async { return 'a'; };
    return await HH\FIXME\UNSAFE_CAST<Awaitable<string>, Awaitable<int>>($a);
}

function builtVec(int $i): vec<string> {
    $v = vec[];
    $v[] = $i;
    return HH\FIXME\UNSAFE_CAST<vec<int>, vec<string>>($v);
}

function dictLiteral(): dict<string, string> {
    $d = dict['a' => 1, 'b' => 2];
    return HH\FIXME\UNSAFE_CAST<dict<string, int>, dict<string, string>>($d);
}

function builtDict(int $i): dict<string, string> {
    $d = dict[];
    $d['a'] = $i;
    return HH\FIXME\UNSAFE_CAST<dict<string, int>, dict<string, string>>($d);
}

function keysetLiteral(): keyset<int> {
    $k = keyset['a', 'b'];
    return HH\FIXME\UNSAFE_CAST<keyset<string>, keyset<int>>($k);
}

function shapeLiteral(): int {
    $p = shape('x' => 1, 'y' => 2);
    return HH\FIXME\UNSAFE_CAST<Point, int>($p);
}

function tupleLiteral(int $i, string $s): int {
    $t = tuple($i, $s);
    return HH\FIXME\UNSAFE_CAST<(int, string), int>($t);
}

function nonEmpty(vec<int> $v): vec<string> {
    if ($v) {
        return HH\FIXME\UNSAFE_CAST<vec<int>, vec<string>>($v);
    }
    return vec[];
}

function truthyString(string $s): int {
    if ($s) {
        return HH\FIXME\UNSAFE_CAST<string, int>($s);
    }
    return 0;
}

function fromDynamic(dynamic $d): int {
    return HH\FIXME\UNSAFE_CAST<dynamic, int>($d);
}

function declaredDynamic(Dog $d): Cat {
    return HH\FIXME\UNSAFE_CAST<dynamic, Cat>($d);
}

function nestedDynamic(vec<Dog> $d): vec<Cat> {
    return HH\FIXME\UNSAFE_CAST<vec<dynamic>, vec<Cat>>($d);
}

function sameTemplate<T>(T $t): int {
    return HH\FIXME\UNSAFE_CAST<T, int>($t);
}

function unboundedTemplate<T>(T $t): int {
    return HH\FIXME\UNSAFE_CAST<mixed, int>($t);
}

function declaredTemplate<T>(int $i, T $_t): T {
    return HH\FIXME\UNSAFE_CAST<T, T>($i);
}

function widenedInLoop(vec<string> $v): int {
    $last = 0;
    foreach ($v as $s) {
        $n = HH\FIXME\UNSAFE_CAST<arraykey, int>($last);
        $last = $s;
        echo $n;
    }
    return 0;
}

// An abstract type constant is only known by its bound.
abstract class Repo {
    abstract const type TId as arraykey;

    public function sameTypeConstant(this::TId $id): int {
        return HH\FIXME\UNSAFE_CAST<this::TId, int>($id);
    }

    public function declaredTypeConstant(int $id): string {
        return HH\FIXME\UNSAFE_CAST<this::TId, string>($id);
    }

    public function declaredThis(Repo $other): int {
        return HH\FIXME\UNSAFE_CAST<this, int>($other);
    }
}
