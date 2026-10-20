// Kouros mark, variant 1в: open green ring + Unbounded "K".
export function Logo({ size = 34 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 48 48" role="img" aria-label="Kouros">
      <circle
        cx="24" cy="24" r="19.5" fill="none" stroke="#3DDC97" strokeWidth="4"
        strokeLinecap="round" strokeDasharray="98 24.5" transform="rotate(-9 24 24)"
      />
      <text
        x="24.4" y="33.2" textAnchor="middle" fill="#F1F3F5"
        fontFamily="Unbounded, sans-serif" fontWeight="700" fontSize="25"
      >
        K
      </text>
    </svg>
  );
}
