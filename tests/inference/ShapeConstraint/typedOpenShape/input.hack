type Labels = shape('name' => string, string...);

function make_labels(): Labels {
	return shape('name' => 'task', 'status' => 'ready');
}

function read_label(Labels $labels, string $key): string {
	return $labels[$key];
}

function named_label(Labels $labels): string {
	return $labels['name'];
}

function invalid_label(): Labels {
	return shape('name' => 'task', 'status' => 42);
}
