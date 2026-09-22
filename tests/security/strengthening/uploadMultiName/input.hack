$files = HH\global_get('_FILES') as KeyedContainer<_, _>;
echo $files['uploads']['name'][0];
