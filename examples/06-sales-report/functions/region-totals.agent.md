# Uses
- [RegionTotal](../records/region-total.agent.md)
- [Sale](../records/sale.agent.md)

# Function: region_totals
Adds up the sales of every region, biggest revenue first.

- in: sales
- out: region totals
- touches: nothing
- steps:
  1. group sales by region into by_region
  2. empty list of RegionTotal into totals
  3. for each region, region_sales in by_region:
     1. add RegionTotal to totals:
        - region
        - orders: count of region_sales
        - revenue: sum of amount in region_sales
  4. return sort totals by revenue descending
