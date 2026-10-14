# Import: ../records/region-total.agent.md
# Import: ../records/sale.agent.md

> Adds up the sales of every region, biggest revenue first.
# Function: region_totals
- in: sales: list of Sale
- out: list of RegionTotal
- effect: pure
  - set by_region = group sales by region
  - set totals = empty list of RegionTotal
  - for each region, rows in by_region:
    - append RegionTotal { region: region, orders: count of rows, revenue: sum of amount in rows } to totals
  - return sort totals by revenue descending
