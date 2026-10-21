final class Item {}
final class Other {}

function accept_item(\HH\class_or_classname<Item> $item): void {}

function check_items(): void {
	accept_item(Item::class);
	accept_item(nameof Item);
	accept_item(Other::class);
	accept_item(nameof Other);
}

function name_from_pointer(): classname<Item> {
	return \HH\class_to_classname(Item::class);
}

function name_from_name(): classname<Item> {
	return \HH\class_to_classname(nameof Item);
}

function pointer_from_name(): class<Item> {
	return \HH\classname_to_class(nameof Item);
}
