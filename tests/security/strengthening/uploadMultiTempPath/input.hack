$files = HH\global_get('_FILES') as KeyedContainer<_, _>;
file_get_contents($files['uploads']['tmp_name'][0]);
