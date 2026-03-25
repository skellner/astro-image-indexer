interface Props {
  search: string;
  imageType: string;
  filterName: string;
  objectName: string;
  filterOptions: string[];
  objectOptions: string[];
  onSearchChange: (v: string) => void;
  onImageTypeChange: (v: string) => void;
  onFilterNameChange: (v: string) => void;
  onObjectNameChange: (v: string) => void;
}

const IMAGE_TYPES = ["Light", "Dark", "Flat", "Bias", "MasterDark", "MasterFlat", "MasterBias"];

export function FilterBar({
  search,
  imageType,
  filterName,
  objectName,
  filterOptions,
  objectOptions,
  onSearchChange,
  onImageTypeChange,
  onFilterNameChange,
  onObjectNameChange,
}: Props) {
  return (
    <div className="flex items-center gap-3 px-4 py-3 bg-gray-850 border-b border-gray-700">
      <input
        type="text"
        placeholder="Search object, file, instrument…"
        value={search}
        onChange={(e) => onSearchChange(e.target.value)}
        className="flex-1 bg-gray-800 border border-gray-700 text-gray-200 placeholder-gray-500 text-sm rounded px-3 py-1.5 focus:outline-none focus:border-blue-500"
      />
      <Select
        value={objectName}
        onChange={onObjectNameChange}
        options={objectOptions}
        placeholder="All objects"
      />
      <Select
        value={imageType}
        onChange={onImageTypeChange}
        options={IMAGE_TYPES}
        placeholder="All types"
      />
      <Select
        value={filterName}
        onChange={onFilterNameChange}
        options={filterOptions}
        placeholder="All filters"
      />
    </div>
  );
}

function Select({
  value,
  onChange,
  options,
  placeholder,
}: {
  value: string;
  onChange: (v: string) => void;
  options: string[];
  placeholder: string;
}) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className="bg-gray-800 border border-gray-700 text-gray-200 text-sm rounded px-3 py-1.5 focus:outline-none focus:border-blue-500"
    >
      <option value="">{placeholder}</option>
      {options.filter(Boolean).map((o) => (
        <option key={o} value={o}>
          {o}
        </option>
      ))}
    </select>
  );
}
