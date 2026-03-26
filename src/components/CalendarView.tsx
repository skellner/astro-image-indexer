import { useMemo, useState } from "react";
import { ImageRow } from "../types";

interface Props {
  images: ImageRow[];
  onSelectDate?: (date: string) => void;
}

const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MONTHS = [
  "January", "February", "March", "April", "May", "June",
  "July", "August", "September", "October", "November", "December",
];

/** Extract YYYY-MM-DD from a date_obs string (handles ISO and space-separated). */
function toDateKey(dateObs: string): string {
  return dateObs.slice(0, 10); // "YYYY-MM-DD"
}

export function CalendarView({ images }: Props) {
  // Default to the month of the most recent image, or current month.
  const defaultMonth = useMemo(() => {
    const dates = images
      .map((img) => img.date_obs)
      .filter((d): d is string => !!d)
      .sort();
    if (dates.length > 0) {
      const latest = dates[dates.length - 1];
      const [y, m] = latest.split("-").map(Number);
      return { year: y, month: m - 1 }; // month is 0-indexed
    }
    const now = new Date();
    return { year: now.getFullYear(), month: now.getMonth() };
  }, [images]);

  const [year, setYear] = useState(defaultMonth.year);
  const [month, setMonth] = useState(defaultMonth.month);

  // Build a map: "YYYY-MM-DD" -> { objects: string[], count: number }
  const dayMap = useMemo(() => {
    const map = new Map<string, { objects: Set<string>; count: number }>();
    for (const img of images) {
      if (!img.date_obs) continue;
      const key = toDateKey(img.date_obs);
      if (!map.has(key)) map.set(key, { objects: new Set(), count: 0 });
      const entry = map.get(key)!;
      if (img.object_name) entry.objects.add(img.object_name);
      entry.count++;
    }
    return map;
  }, [images]);

  function prevMonth() {
    if (month === 0) { setMonth(11); setYear((y) => y - 1); }
    else setMonth((m) => m - 1);
  }
  function nextMonth() {
    if (month === 11) { setMonth(0); setYear((y) => y + 1); }
    else setMonth((m) => m + 1);
  }

  // Calendar grid: days in the month, offset by weekday (Mon=0)
  const firstDow = new Date(year, month, 1).getDay(); // 0=Sun
  const startOffset = (firstDow + 6) % 7; // shift so Mon=0
  const daysInMonth = new Date(year, month + 1, 0).getDate();
  const totalCells = Math.ceil((startOffset + daysInMonth) / 7) * 7;

  const today = new Date();
  const todayKey = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, "0")}-${String(today.getDate()).padStart(2, "0")}`;

  return (
    <div className="flex flex-col flex-1 min-h-0 overflow-auto p-4">
      {/* Month navigation */}
      <div className="flex items-center gap-4 mb-4">
        <button
          onClick={prevMonth}
          className="px-3 py-1 rounded bg-gray-800 hover:bg-gray-700 text-gray-300 text-sm transition-colors"
        >
          ‹
        </button>
        <h2 className="text-base font-semibold text-gray-200 w-40 text-center">
          {MONTHS[month]} {year}
        </h2>
        <button
          onClick={nextMonth}
          className="px-3 py-1 rounded bg-gray-800 hover:bg-gray-700 text-gray-300 text-sm transition-colors"
        >
          ›
        </button>
      </div>

      {/* Grid */}
      <div className="grid grid-cols-7 gap-px bg-gray-800 rounded-lg overflow-hidden flex-1">
        {/* Weekday headers */}
        {WEEKDAYS.map((wd) => (
          <div
            key={wd}
            className="bg-gray-900 text-gray-500 text-xs font-medium text-center py-2"
          >
            {wd}
          </div>
        ))}

        {/* Day cells */}
        {Array.from({ length: totalCells }).map((_, i) => {
          const dayNum = i - startOffset + 1;
          const isInMonth = dayNum >= 1 && dayNum <= daysInMonth;
          const dateKey = isInMonth
            ? `${year}-${String(month + 1).padStart(2, "0")}-${String(dayNum).padStart(2, "0")}`
            : null;
          const entry = dateKey ? dayMap.get(dateKey) : undefined;
          const isToday = dateKey === todayKey;

          return (
            <div
              key={i}
              className={`bg-gray-950 min-h-[80px] p-1.5 flex flex-col ${
                !isInMonth ? "opacity-20" : ""
              }`}
            >
              {/* Day number */}
              <span
                className={`text-xs font-medium self-start leading-none mb-1 w-5 h-5 flex items-center justify-center rounded-full ${
                  isToday
                    ? "bg-blue-600 text-white"
                    : "text-gray-500"
                }`}
              >
                {isInMonth ? dayNum : ""}
              </span>

              {/* Objects */}
              {entry && (
                <div className="flex flex-col gap-0.5 flex-1 min-h-0">
                  {/* Count badge */}
                  <span className="text-[10px] text-gray-500 leading-none mb-0.5">
                    {entry.count} frame{entry.count !== 1 ? "s" : ""}
                  </span>
                  {/* Object names */}
                  {Array.from(entry.objects).slice(0, 3).map((obj) => (
                    <span
                      key={obj}
                      className="text-[11px] leading-tight bg-blue-900/50 text-blue-300 rounded px-1 py-0.5 truncate"
                      title={obj}
                    >
                      {obj}
                    </span>
                  ))}
                  {entry.objects.size > 3 && (
                    <span className="text-[10px] text-gray-500 leading-none">
                      +{entry.objects.size - 3} more
                    </span>
                  )}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
