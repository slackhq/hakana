interface IP {}
final class SA implements IP {}
final class SP implements IP {}

enum class PC: IP {
	SP a = new SP();
	SA b = new SA();
}

function foo(HH\MemberOf<PC, IP> $p): ?SA {
    if ($p is SA) {
        return $p;
    }
	return null;
}
