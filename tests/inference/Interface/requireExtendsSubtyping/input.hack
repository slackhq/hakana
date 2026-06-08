abstract class Node {
	public function getCode(): string {
		return '';
	}
}

interface ITypeSpecifier {
	require extends Node;
}

interface ISimpleSpecifier extends ITypeSpecifier {}

function direct_req(ITypeSpecifier $x): ?Node {
	return $x;
}

function inherited_req(ISimpleSpecifier $x): ?Node {
	return $x;
}
